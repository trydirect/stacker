//! Every binary the stacker image ships is built and copied on both paths into
//! the image: the CI job (`.github/workflows/docker.yml` compiles, archives,
//! unpacks, and the Dockerfile copies it in as `prebuilt`) and a local
//! `docker build` (the Dockerfile's own `builder` stage).
//!
//! `bake` turns a deployed build box into a Quick Deploy snapshot. It was left
//! out of the image, so every bake meant compiling it by hand and `docker cp`
//! into the running production container.

use std::fs;
use std::path::PathBuf;

/// The binaries in /app of the runtime image.
const IMAGE_BINARIES: &[&str] = &[
    "server",
    "console",
    "cleanup-notify",
    "backfill_field_policy",
    "bake",
];

fn read(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {file}: {err}"))
}

/// Lines that are not comments, so commented-out history does not count.
fn active(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_dockerfile_builds_and_ships_every_image_binary() {
    let dockerfile = active(&read("Dockerfile"));
    for bin in IMAGE_BINARIES {
        assert!(
            dockerfile
                .iter()
                .any(|l| l.contains("cargo build --release") && l.contains(&format!("--bin {bin}"))),
            "builder stage must compile `{bin}`"
        );
        assert!(
            dockerfile.iter().any(
                |l| l.trim() == format!("COPY --from=builder /app/target/release/{bin} /{bin}")
            ),
            "builder-source must take `{bin}` from the builder stage"
        );
        assert!(
            dockerfile
                .iter()
                .any(|l| l.trim() == format!("COPY --from=binaries /{bin} .")),
            "the runtime image must contain `{bin}`"
        );
    }
}

#[test]
fn ci_builds_archives_and_unpacks_every_image_binary() {
    let workflow = active(&read(".github/workflows/docker.yml")).join("\n");
    for bin in IMAGE_BINARIES {
        assert!(
            workflow.contains(&format!("--bin {bin}")),
            "CI must compile `{bin}`"
        );
        assert!(
            workflow.contains(&format!("cp target/release/{bin} app/stacker/{bin}")),
            "CI must put `{bin}` in the artifact"
        );
        assert!(
            workflow.contains(&format!("stacker/{bin}")),
            "the docker job must unpack `{bin}` from the artifact"
        );
    }
    let chmod = workflow
        .lines()
        .find(|l| l.trim_start().starts_with("chmod +x"))
        .expect("the docker job makes the binaries executable");
    for bin in IMAGE_BINARIES {
        assert!(
            chmod.split_whitespace().any(|word| word == *bin),
            "`{bin}` must be made executable: {chmod}"
        );
    }
}
