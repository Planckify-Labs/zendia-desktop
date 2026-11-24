/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let output = out_dir.join("HubProtocol.rs");
    let depfile = out_dir.join("HubProtocol.inputs");

    let status = Command::new("python3")
        .arg(manifest_dir.join("generate.py"))
        .arg("--messages")
        .arg(manifest_dir.join("messages.txt"))
        .arg("--handwritten")
        .arg(manifest_dir.join("src/ipc_types.rs"))
        .arg("--output")
        .arg(&output)
        .arg("--depfile")
        .arg(&depfile)
        .status()?;
    if !status.success() {
        return Err("Generating the Hub protocol failed".into());
    }

    println!("cargo:rerun-if-changed={}", manifest_dir.join("generate.py").display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("src/ipc_types.rs").display()
    );
    for input in std::fs::read_to_string(&depfile)?.lines() {
        println!("cargo:rerun-if-changed={input}");
    }
    Ok(())
}
