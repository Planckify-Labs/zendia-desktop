/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What pages get from the browser's look: the system theme, whose selection colors pages paint their selection
//! with, and a user style sheet for the scrollbars.
//!
//! Both are Zendia's green, the color of INSERT. Light content gets the green opaque, which the engine turns into a
//! tint that looks like it over white, so dark text stays readable on it. Dark content gets it translucent, a dark
//! green that keeps light text readable.

use hub_protocol::Core_AnonymousBuffer;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, OnceLock};

/// INSERT's green.
const BRAND_GREEN: u32 = 0xffc3_e88d;

/// The user style every page gets. Scrollbars of pages that don't color their own take the green: the brand green on
/// dark pages, and a deeper green of the same family on light ones, where the light one would barely show.
pub const PAGE_USER_STYLE: &str = ":root { scrollbar-color: light-dark(#9ece6a, #c3e88d) light-dark(rgb(158 206 106 / 0.14), rgb(195 232 141 / 0.12)); }";

/// The color roles of `Gfx::SystemTheme`, in the order of `ENUMERATE_COLOR_ROLES` in `Libraries/LibGfx/SystemTheme.h`,
/// with `Base/res/themes/Default.ini`'s colors as ARGB, but for the selections. A role that file leaves out is black,
/// as `Gfx::load_system_theme()` makes it.
const COLOR_ROLES: [(&str, u32); 36] = [
    ("Accent", 0xffab_6e4a),
    ("Base", 0xffff_ffff),
    ("BaseText", 0xff00_0000),
    ("Black", 0xff00_0000),
    ("Blue", 0xff00_0000),
    ("Button", 0xffd4_d0c8),
    ("ButtonText", 0xff00_0000),
    ("ColorSchemeBackground", 0xff00_0000),
    ("ColorSchemeForeground", 0xff00_0000),
    ("Cyan", 0xff00_0000),
    ("Green", 0xff00_0000),
    ("Gutter", 0xffd4_d0c8),
    // A window in the background keeps a quieter green on what it has selected.
    ("InactiveSelection", 0xffd9_e8c4),
    ("LightSelection", BRAND_GREEN),
    // 40% of the green: over a dark page it is a dark green that light text stays readable on.
    ("DarkSelection", 0x66c3_e88d),
    ("Link", 0xff00_00ff),
    ("Magenta", 0xff00_0000),
    ("PlaceholderText", 0xff80_8080),
    ("Red", 0xff00_0000),
    ("Selection", BRAND_GREEN),
    ("SyntaxComment", 0xffa0_a1a7),
    ("SyntaxControlKeyword", 0xffa4_2ea2),
    ("SyntaxIdentifier", 0xff00_0000),
    ("SyntaxKeyword", 0xffa4_2ea2),
    ("SyntaxNumber", 0xff97_6715),
    ("SyntaxOperator", 0xff00_0000),
    ("SyntaxPreprocessorStatement", 0xff00_a0a0),
    ("SyntaxPunctuation", 0xff00_0000),
    ("SyntaxString", 0xff53_a053),
    ("SyntaxType", 0xffa0_00a0),
    ("ThreedShadow1", 0xff80_8080),
    ("Tooltip", 0xffff_ffe1),
    ("TooltipText", 0xff00_0000),
    ("White", 0xff00_0000),
    ("Window", 0xffd4_d0c8),
    ("Yellow", 0xff00_0000),
];

/// The bytes of a `Gfx::SystemTheme`: a `u32` per color role, then a `bool` per flag role, padded to the struct's
/// alignment. Both enums start with `NoRole`, which takes the first slot of each.
fn system_theme_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((COLOR_ROLES.len() + 1) * 4 + 4);
    bytes.extend_from_slice(&0u32.to_ne_bytes());
    for (_, color) in COLOR_ROLES {
        bytes.extend_from_slice(&color.to_ne_bytes());
    }
    // NoRole, then IsDark: the palette is a light one, as Default.ini's.
    bytes.extend_from_slice(&[0, 0]);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

/// The system theme every view gets, in shared memory made once.
pub fn system_theme() -> Core_AnonymousBuffer {
    static THEME: OnceLock<Option<(u64, Arc<OwnedFd>)>> = OnceLock::new();
    let shared_memory = THEME
        .get_or_init(|| match shared_memory_with(&system_theme_bytes()) {
            Ok(memory) => Some(memory),
            Err(error) => {
                eprintln!("zendia: Unable to make the pages' system theme: {error}");
                None
            }
        })
        .clone();
    Core_AnonymousBuffer { shared_memory }
}

fn shared_memory_with(bytes: &[u8]) -> std::io::Result<(u64, Arc<OwnedFd>)> {
    // SAFETY: The name is a valid C string, and a descriptor memfd_create() returns is ours to own.
    let fd = unsafe {
        libc::memfd_create(
            c"zendia-system-theme".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `fd` is a new descriptor that nothing else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes)?;
    file.seek(SeekFrom::Start(0))?;
    // The engine maps the buffer only once its size can no longer change.
    // SAFETY: fcntl() on a descriptor we own, with flags it takes.
    if unsafe {
        libc::fcntl(
            file.as_raw_fd(),
            libc::F_ADD_SEALS,
            libc::F_SEAL_GROW | libc::F_SEAL_SHRINK,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok((bytes.len() as u64, Arc::new(OwnedFd::from(file))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_roles_follow_the_engine() {
        let header = include_str!("../../../../Libraries/LibGfx/SystemTheme.h");
        let start = header.find("#define ENUMERATE_COLOR_ROLES").expect("color roles");
        let end = start + header[start..].find("\n\n").expect("end of color roles");
        let roles: Vec<&str> = header[start..end]
            .split("C(")
            .skip(1)
            .map(|each| each.split(')').next().unwrap_or_default())
            .collect();
        let ours: Vec<&str> = COLOR_ROLES.iter().map(|(name, _)| *name).collect();
        assert_eq!(ours, roles);
        assert!(header.contains("C(IsDark)\n"), "IsDark is the one flag role");
        assert!(
            header.contains("enum class ColorRole {\n    NoRole,")
                && header.contains("enum class FlagRole {\n    NoRole,"),
            "both role enums start with NoRole"
        );
    }

    #[test]
    fn theme_is_the_size_of_the_struct() {
        // 37 colors with NoRole, two flags with it, padded to four bytes.
        assert_eq!(system_theme_bytes().len(), 37 * 4 + 4);
    }
}
