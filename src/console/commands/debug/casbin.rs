use crate::configuration::get_configuration;
use actix_web::{rt, post, web, HttpResponse, Result, http::header::ContentType};

pub struct CasbinCommand {
    action: String,
    method: String,
    subject: String
}

impl CasbinCommand {
    pub fn new(action: String, method: String, subject: String) -> Self {
        Self { action, method, subject }
    }
}

impl crate::console::commands::CallableTrait for CasbinCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("action: {}, method: {}, subject: {}", self.action, self.method, self.subject);
        Ok(())
    }
}
