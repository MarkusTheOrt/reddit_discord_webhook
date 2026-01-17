//! Apply Database migrations, making it easy to deploy this bot on a new
//! install
//!
//! ## usage
//!
//! Make sure your DATABASE_URL and DATABASE_TOKEN env variables are set.
//! (You can set these as env variables or use a .env file in your working dir)
//!
//! execute this file by using `cargo run --bin migrate`

use std::io::Read;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    _ = dotenvy::dotenv();

    let mut files = Vec::with_capacity(1);
    let dir = std::fs::read_dir("migrations/")?;
    for entry in dir {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        files.push((name.to_str().unwrap().to_owned(), entry.path()));
    }

    _ = std::fs::create_dir("database/");
    let lbsqlc = libsql::Builder::new_local("database/db").build().await?;
    let conn = lbsqlc.connect()?;
    for (name, path) in files {
        println!("Running migration: {name}");
        let mut file = std::fs::File::open(path)?;
        let file_meta = file.metadata()?;
        let mut str = String::with_capacity(file_meta.len() as usize);
        _ = file.read_to_string(&mut str)?;
        _ = conn.execute_batch(&str).await?;
    }

    println!("\n✅ All migrations applied successfully!");
    Ok(())
}
