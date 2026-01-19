//! Simple Authentication Example
//!
//! This example demonstrates basic authentication flow with JWT and passwords.
//!
//! Run with: `cargo run --example simple_auth`

use oxify_authn::{JwtConfig, JwtManager, PasswordManager, Permission, User};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Simple Authentication Example ===\n");

    // 1. JWT Authentication
    println!("1. JWT Authentication");
    let config = JwtConfig::development();
    let jwt_manager = JwtManager::new(&config)?;

    let user = User {
        username: "alice".to_string(),
        roles: vec!["admin".to_string()],
        email: Some("alice@example.com".to_string()),
        full_name: Some("Alice Wonderland".to_string()),
        last_login: None,
        permissions: vec![Permission::Admin],
    };

    let token = jwt_manager.generate_token(&user)?;
    println!("   ✓ Token generated");

    let validation = jwt_manager.validate_token(&token)?;
    println!(
        "   ✓ Token validated for user: {}\n",
        validation.user.username
    );

    // 2. Password Management
    #[cfg(feature = "password")]
    {
        println!("2. Password Management");
        let password_manager = PasswordManager::new();

        let password = "MySecurePassword123!";
        let hash = password_manager.hash_password(password)?;
        println!("   ✓ Password hashed");

        let is_valid = password_manager.verify_password(password, &hash)?;
        println!("   ✓ Password verified: {is_valid}\n");
    }

    println!("=== Example completed successfully! ===");
    Ok(())
}
