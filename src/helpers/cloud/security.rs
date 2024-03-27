use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key // Or `Aes128Gcm`
};
use base64::{engine::general_purpose, Engine as _};
use redis::{Commands, Connection};
use tracing::Instrument;

#[derive(Debug, Default, PartialEq, Clone)]
pub(crate) struct Secret {
    pub(crate) user_id: String,
    pub(crate) project_id: i32,
    pub(crate) field: String, // cloud_token/cloud_key/cloud_secret
    pub(crate) nonce: Vec<u8>,
}


impl Secret {
    pub fn new() -> Self {

        Secret {
            user_id: "".to_string(),
            project_id: 0,
            field: "".to_string(),
            nonce: vec![],
        }
    }
    #[tracing::instrument(name = "Secret::connect_storage")]
    fn connect_storage() -> Connection {
        match redis::Client::open("redis://127.0.0.1/"){
            Ok(client) => {
                match client.get_connection() {
                    Ok(connection) => connection,
                    Err(_err) => panic!("Error connecting Redis")
                }
            }
            Err(err) => panic!("Could not connect to Redis, {:?}", err)
        }
    }

    #[tracing::instrument(name = "Secret::save")]
    fn save(&self, value: &[u8]) -> &Self {
        let mut conn = Secret::connect_storage();
        let key = format!("{}_{}_{}", self.user_id, self.project_id, self.field);
        tracing::debug!("Saving into storage..");
        let _: () = match conn.set(key, value) {
            Ok(s) => s,
            Err(e) => panic!("Could not save to storage {}", e)
        };
        self
    }

    pub fn b64_encode(value: &Vec<u8>) -> String {
        general_purpose::STANDARD.encode(value)
    }

    pub fn b64_decode(value: &String) -> Result<Vec<u8>, String> {
        general_purpose::STANDARD.decode(value)
            .map_err(|e| format!("b64_decode error {}", e))
    }

    #[tracing::instrument(name = "Secret::get")]
    fn get(&mut self, key: String) -> &mut Self {
        let mut conn = Secret::connect_storage();
        let nonce: Vec<u8> = match conn.get(&key) {
            Ok(value) => {
                tracing::debug!("Got value from storage {:?}", &value);
                value
            },
            Err(_e) => {
                tracing::error!("Could not get value from storage by key {:?} {:?}", &key,  _e);
                vec![]
            }
        };

        self.nonce = nonce;
        self
    }

    #[tracing::instrument(name = "encrypt.")]
    pub fn encrypt(&self, token: String) -> Result<Vec<u8>, String> {

        // let sec_key = std::env::var("SECURITY_KEY")
        //     .expect("SECURITY_KEY environment variable is not set")
        //     .as_bytes();
        let sec_key = "SECURITY_KEY_SHOULD_BE_OF_LEN_32".as_bytes();
        // let key = Aes256Gcm::generate_key(OsRng);
        let key: &Key::<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(&sec_key);
        // eprintln!("encrypt key {key:?}");
        // eprintln!("encrypt: from slice key {key:?}");
        let cipher = Aes256Gcm::new(&key);
        // eprintln!("encrypt: Cipher str {cipher:?}");
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
        eprintln!("Nonce bytes {nonce:?}");
        // let nonce_b64: String = general_purpose::STANDARD.encode(nonce);
        // eprintln!("Nonce b64 {nonce_b64:?}");
        eprintln!("token {token:?}");

        let cipher_vec = cipher.encrypt(&nonce, token.as_ref())
            .map_err(|e| format!("{:?}", e))?;

        // store nonce for a limited amount of time
        // self.save(cipher_vec.clone());
        self.save(nonce.as_slice());

        eprintln!("Cipher {cipher_vec:?}");
        Ok(cipher_vec)
    }

    #[tracing::instrument(name = "decrypt.")]
    pub fn decrypt(&mut self, encrypted_data: Vec<u8>) -> Result<String, String> {
        let sec_key = "SECURITY_KEY_SHOULD_BE_OF_LEN_32".as_bytes();
        // let sec_key = std::env::var("SECURITY_KEY")
        //     .expect("SECURITY_KEY environment variable is not set")
        //     .as_bytes();
        let key: &Key::<Aes256Gcm> = Key::<Aes256Gcm>::from_slice(&sec_key);
        // eprintln!("decrypt: Key str {key:?}");
        let rkey = format!("{}_{}_{}", self.user_id, self.project_id, self.field);
        eprintln!("decrypt: Key str {rkey:?}");
        self.get(rkey);
        // eprintln!("decrypt: nonce b64:decoded {nonce:?}");

        let nonce = Nonce::from_slice(self.nonce.as_slice());
        eprintln!("decrypt: nonce {nonce:?}");

        let cipher = Aes256Gcm::new(&key);
        // eprintln!("decrypt: Cipher str {cipher:?}");
        eprintln!("decrypt: str {encrypted_data:?}");

        let plaintext = cipher.decrypt(&nonce, encrypted_data.as_ref())
            .map_err(|e| format!("{:?}", e))?;

        Ok(String::from_utf8(plaintext).map_err(|e| format!("{:?}", e))?)
    }
}