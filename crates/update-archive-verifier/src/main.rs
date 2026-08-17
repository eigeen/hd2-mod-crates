use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let (archive_path, public_key) = parse_arguments()?;
    verify_signed_archive(&archive_path, public_key)?;
    println!("verified {}", archive_path.display());
    Ok(())
}

fn parse_arguments() -> Result<(PathBuf, [u8; 32]), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let archive_path = required_argument(arguments.next(), "signed ZIP path")?;
    let encoded_key = required_argument(arguments.next(), "public key hex")?;
    if arguments.next().is_some() {
        return Err(invalid_input("expected exactly two arguments").into());
    }
    let encoded_key = encoded_key
        .to_str()
        .ok_or_else(|| invalid_input("public key must be UTF-8"))?;
    Ok((archive_path.into(), decode_public_key(encoded_key)?))
}

fn required_argument(value: Option<OsString>, name: &str) -> Result<OsString, io::Error> {
    value.ok_or_else(|| invalid_input(&format!("missing {name}")))
}

fn verify_signed_archive(path: &Path, public_key: [u8; 32]) -> Result<(), Box<dyn Error>> {
    let context = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_input("archive filename must be UTF-8"))?;
    let keys = zipsign_api::verify::collect_keys(std::iter::once(Ok::<_, io::Error>(public_key)))?;
    let mut archive = File::open(path)?;
    zipsign_api::verify::verify_zip(&mut archive, &keys, Some(context.as_bytes()))?;
    Ok(())
}

fn decode_public_key(encoded: &str) -> Result<[u8; 32], io::Error> {
    if encoded.len() != 64 {
        return Err(invalid_input(
            "public key must contain 64 hexadecimal characters",
        ));
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
            .map_err(|_| invalid_input("public key contains non-hexadecimal characters"))?;
    }
    Ok(key)
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_public_key() {
        let encoded = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        assert_eq!(
            decode_public_key(encoded).unwrap(),
            std::array::from_fn(|index| index as u8)
        );
    }

    #[test]
    fn rejects_an_invalid_public_key() {
        assert!(decode_public_key("abcd").is_err());
        assert!(decode_public_key(&"z".repeat(64)).is_err());
    }
}
