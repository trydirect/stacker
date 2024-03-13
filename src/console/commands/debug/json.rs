use crate::configuration::get_configuration;
use actix_web::rt;
use actix_web::web;
use sqlx::PgPool;

pub struct JsonCommand {
    line: usize,
    column: usize,
    payload: String
}

impl JsonCommand {
    pub fn new(line: usize, column: usize, payload: String) -> Self {
        Self { line, column, payload }
    }
}

impl crate::console::commands::CallableTrait for JsonCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let payload = std::fs::read_to_string(&self.payload)?;
        println!("line={} column={} payload={}", self.line, self.column, payload);
        Ok(())
    }
}
