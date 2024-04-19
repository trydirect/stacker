use actix_web::{rt, Result};
use crate::helpers::dockerhub::DockerHub;
use crate::forms::project::DockerImage;

use tracing_subscriber::FmtSubscriber;

pub struct DockerhubCommand {
    json: String,
}

impl DockerhubCommand {
    pub fn new(json: String) -> Self {
        Self { json }
    }
}

impl crate::console::commands::CallableTrait for DockerhubCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");


        rt::System::new().block_on(async {
            println!("{}", self.json);
            let dockerImage: DockerImage = serde_json::from_str(&self.json)?;
            let mut dockerhub = DockerHub::try_from(&dockerImage)?;
            /*
            if dockerhub.creds.username.is_empty() {
            */
            let lookupOfficialRepos = dockerhub.lookup_official_repos();
            let lookupPublicRepos = dockerhub.lookup_public_repos();
            let lookupPublicRepos1 = dockerhub.lookup_public_repos();
            tokio::select! {
                Ok(true) = lookupOfficialRepos => {println!("lookup oficial repos"); }
                Ok(true) = lookupPublicRepos => {println!("lookup public repos"); }
                Ok(true) = lookupPublicRepos1 => {println!("lookup public repos1"); }
                //todo add timer
                //complete => { println!("complete"); }
                //default => { println!("no good result"); }
                else => { println!("nothing found") }
            }
            /*
                if Ok(true) == dockerhub.lookup_official_repos().await {
                    println!("official: true");
                    return Ok(());
                } else {
                    println!("official: false");
                };

                if Ok(true) == dockerhub.lookup_public_repos().await {
                    println!("public: true");
                    return Ok(());
                };
                */
                /*
            } else {
                println!("username is not empty");
                if Ok(true) == dockerhub.lookup_private_repo().await {
                    tracing::debug!("private: true");
                    return Ok(());
                };
            }
            */

            Ok(())
        })
    }
}
