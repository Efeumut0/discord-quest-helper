use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: verify_update <setup.exe> <setup.exe.sig>".into());
    }
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
    let encoded_key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("Missing updater public key")?;
    let key_text = String::from_utf8(STANDARD.decode(encoded_key.trim())?)?;
    let signature_text =
        String::from_utf8(STANDARD.decode(std::fs::read_to_string(&args[1])?.trim())?)?;
    let key = PublicKey::decode(&key_text)?;
    let signature = Signature::decode(&signature_text)?;
    let mut bytes = std::fs::read(&args[0])?;
    key.verify(&bytes, &signature, false)?;
    if bytes.is_empty() {
        return Err("Empty installer".into());
    }
    let middle = bytes.len() / 2;
    bytes[middle] ^= 1;
    if key.verify(&bytes, &signature, false).is_ok() {
        return Err("A modified installer passed signature verification".into());
    }
    println!("PASS: setup matches the embedded public key; modified bytes are rejected");
    Ok(())
}
