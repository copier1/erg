#![allow(deprecated)]

use std::env;
use std::fs;
use std::path;

fn main() -> std::io::Result<()> {
    // Create a ".erg" directory
    let erg_path = env::home_dir()
        .expect("failed to get the location of the home dir")
        .to_str()
        .expect("invalid encoding of the home dir name")
        .to_string()
        + "/.erg";
    if !path::Path::new(&erg_path).exists() {
        fs::create_dir(&erg_path)?;
    }
    println!("cargo:rustc-env=CARGO_ERG_PATH={erg_path}");
    // create a std library in ".erg"
    copy_dir(&erg_path, "lib")?;
    Ok(())
}

fn copy_dir(erg_path: &str, path: &str) -> std::io::Result<()> {
    let full_path = format!("{erg_path}/{path}");
    if !path::Path::new(&full_path).exists() {
        fs::create_dir(&full_path)?;
    }
    let mut dirs = vec![];
    for res in fs::read_dir(path)? {
        let entry = res?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            dirs.push(entry);
        } else {
            let filename = entry_path
                .file_name()
                .expect("this is not a file")
                .to_str()
                .unwrap();
            let filename = format!("{full_path}/{filename}");
            fs::copy(&entry_path, filename)?;
        }
    }
    for dir in dirs {
        copy_dir(erg_path, dir.path().to_str().unwrap())?;
    }
    Ok(())
}
