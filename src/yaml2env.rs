use stacker::configuration::get_configuration;
use std::fs::File;
use std::io::Write;

fn main() -> std::io::Result<()> {
    let configuration = get_configuration().expect("Failed to read configuration.");
    let mut file = File::create(".env")?;

    writeln!(&mut file, "DB_USER={}", configuration.database.username)?;
    writeln!(&mut file, "DB_PASSWORD={}", configuration.database.password)?;
    writeln!(
        &mut file,
        "DB_NAME={}",
        configuration.database.database_name
    )?;
    writeln!(&mut file, "DB_PORT={}", configuration.database.port)?;

    Ok(())
}
