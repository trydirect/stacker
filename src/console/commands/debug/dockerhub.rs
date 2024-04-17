use actix_web::{rt, Result};

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
        rt::System::new().block_on(async {
            println!("{}", self.json);

            Ok(())
        })
    }
}
