use anyhow::Result;
use toki_storage::{default_key_path, generate_key, save_key_to_file, Database};

/// Initialize toki with encryption setup
///
/// # Errors
///
/// Returns an error if key generation, file operations, or database initialization fails
pub fn init_command(enable_encryption: bool) -> Result<()> {
    println!("Initializing Time Tracking Daemon...\n");

    if enable_encryption {
        println!("Setting up encryption...");

        let key_path = default_key_path();

        // Check if key already exists
        if key_path.exists() {
            println!(
                "Error: Encryption key already exists at: {}",
                key_path.display()
            );
            println!("If you want to reinitialize, please delete the existing key file first.");
            println!("WARNING: This will make existing encrypted data inaccessible!");
            return Ok(());
        }

        // Generate and save encryption key
        let key = generate_key();
        save_key_to_file(&key, &key_path)?;

        println!("Encryption key generated and saved to:");
        println!("  {}", key_path.display());
        println!("  Keep this file safe! Loss of this key means data cannot be recovered.");

        // Initialize encrypted database
        let db = Database::new_with_encryption(None, Some(key))?;
        drop(db);

        println!("Encrypted database initialized");
    } else {
        println!("Warning: Encryption disabled - data will be stored in plaintext");
        println!("Run 'toki init --encrypt' to enable encryption");

        // Initialize unencrypted database
        let db = Database::new(None)?;
        drop(db);

        println!("Database initialized");
    }

    println!("\nSetup complete! You can now start tracking:");
    println!("  toki start");

    Ok(())
}
