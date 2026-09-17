//! Minting, hashing and verification of the agent bearer token.
//!
//! Stacker stores a *digest* of the token, not the token itself, so
//! authenticating an agent no longer requires reading every agent's secret out
//! of Vault. Vault remains the distribution channel — the agent polls it and
//! adopts rotations — so whatever writes a token there must write the digest
//! here in the same operation. `services::agent_token::issue` is that place.

use rand::Rng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Characters the token is drawn from: base64url's alphabet, 64 symbols, so
/// `gen_range(0..64)` is unbiased.
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Token length in characters. 86 symbols over a 64-symbol alphabet is 516
/// bits of entropy, which is what justifies a plain digest below.
const TOKEN_LEN: usize = 86;

/// Algorithm tag stored alongside the digest, so a future change of hash is
/// detectable at read time instead of silently mis-verifying.
const SHA256_TAG: &str = "sha256:";

/// Mint a fresh bearer token.
///
/// The single home for this: it used to be duplicated byte-for-byte in
/// `routes::agent::register` and `routes::agent::link`.
pub fn generate() -> String {
    let mut rng = rand::thread_rng();
    (0..TOKEN_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Digest to store in `agents.token_hash`.
///
/// SHA-256 rather than a slow KDF, deliberately. Argon2 and friends exist to
/// make guessing a low-entropy, human-chosen secret expensive; there is nothing
/// to guess in 516 bits of CSPRNG output. The cost would be real, though:
/// `try_agent` runs on every long-poll, heartbeat and report, so a
/// ~50-100ms verification per request would be a self-inflicted denial of
/// service.
///
/// That argument holds only while tokens are minted by [`generate`]. If an
/// operator-supplied value could ever be stored, an unsalted digest becomes a
/// rainbow-table target — which is why `rotate_token` mints server-side.
pub fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{SHA256_TAG}{digest:x}")
}

/// Whether `presented` is the token behind `stored`.
///
/// Constant-time: the comparison is over digests rather than secrets, so a
/// timing oracle would leak digest bytes rather than the token, but the cost
/// of doing it properly is one dependency line.
///
/// Returns `false` — never an error — for a stored value that is empty,
/// malformed, or carries an unknown algorithm tag. A credential that cannot be
/// interpreted must not authenticate.
pub fn verify(presented: &str, stored: &str) -> bool {
    let Some(expected_hex) = stored.strip_prefix(SHA256_TAG) else {
        return false;
    };
    if expected_hex.len() != 64 {
        return false;
    }

    let actual = Sha256::digest(presented.as_bytes());
    let Ok(expected) = decode_hex_32(expected_hex) else {
        return false;
    };

    actual.as_slice().ct_eq(&expected).into()
}

fn decode_hex_32(hex: &str) -> Result<[u8; 32], ()> {
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for (i, chunk) in bytes.chunks(2).enumerate() {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_have_the_expected_shape() {
        let token = generate();
        assert_eq!(token.chars().count(), TOKEN_LEN);
        assert!(
            token.bytes().all(|b| CHARSET.contains(&b)),
            "token must stay within the base64url alphabet: {token}"
        );
        assert_ne!(generate(), generate(), "tokens must not repeat");
    }

    #[test]
    fn hash_is_tagged_and_stable() {
        let h = hash("some-token");
        assert!(
            h.starts_with("sha256:"),
            "digest must carry its algorithm: {h}"
        );
        assert_eq!(h.len(), "sha256:".len() + 64);
        assert_eq!(h, hash("some-token"));
        assert_ne!(h, hash("some-tokem"));
    }

    #[test]
    fn verify_accepts_the_round_trip() {
        let token = generate();
        assert!(verify(&token, &hash(&token)));
    }

    #[test]
    fn verify_rejects_a_single_character_change() {
        let token = generate();
        let stored = hash(&token);
        let mut altered = token.clone();
        altered.pop();
        altered.push(if token.ends_with('A') { 'B' } else { 'A' });
        assert!(!verify(&altered, &stored));
    }

    /// A stored value that cannot be interpreted must never authenticate —
    /// this is the failure mode that matters, since `token_hash` is nullable
    /// and its content comes from the database.
    #[test]
    fn verify_rejects_unusable_stored_values() {
        let token = generate();
        for stored in [
            "",
            "   ",
            "not-a-digest",
            // right shape, no tag
            &hash(&token)[7..],
            // unknown algorithm
            &format!("md5:{}", &hash(&token)[7..]),
            // tagged but truncated
            "sha256:abcd",
            // tagged, right length, not hex
            &format!("sha256:{}", "z".repeat(64)),
        ] {
            assert!(
                !verify(&token, stored),
                "must not authenticate against stored value {stored:?}"
            );
        }
    }
}
