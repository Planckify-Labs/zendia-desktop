/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library button's icon: a strip of frames that plays while the pointer is over the button and plays back when
//! it leaves. The first frame is the icon at rest.

use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::theme::motion;
use freya::animation::*;
use freya::prelude::*;
use std::sync::LazyLock;

/// Frames side by side in the `library-sprite` icon.
const FRAMES: u32 = 36;
/// Width and height of one frame.
const FRAME_SIZE: f32 = 18.0;

/// The sprite as [`drawable_sprite`] rewrites it.
static DRAWABLE_SPRITE: LazyLock<Option<Bytes>> = LazyLock::new(|| {
    let (_, bytes) = prepared_icon_bytes("library-sprite")?;
    Some(Bytes::from(drawable_sprite(std::str::from_utf8(&bytes).ok()?)))
});

/// Rewrites what the renderer cannot draw as the file says it: it ignores the style sheet and the masks, and without
/// a `viewBox` it draws one file unit per screen pixel, so the frames drift on a scaled screen.
fn drawable_sprite(svg: &str) -> String {
    let size = r#"width="648" height="18""#;
    let svg = svg
        .replacen(size, &format!(r#"viewBox="0 0 648 18" {size}"#), 1)
        // The style sheet keeps the glow shapes invisible in the light scheme.
        .replace(
            r#"class="zen-library-glow"#,
            r#"display="none" class="zen-library-glow"#,
        )
        .replace(r#"mask="url("#, r#"clip-path="url("#);
    // Each mask hides everything below one height, which a clip path states as well.
    let mut parts = svg.split("<mask ");
    let mut drawable = parts.next().unwrap_or_default().to_string();
    for part in parts {
        let Some((mask, rest)) = part.split_once("</mask>") else {
            drawable.push_str("<mask ");
            drawable.push_str(part);
            continue;
        };
        let id = mask.split('"').nth(1).unwrap_or_default();
        let edge = mask
            .rsplit_once(r#" y=""#)
            .and_then(|(_, tail)| tail.split('"').next())
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(30.0);
        let height = edge + 10.0;
        drawable.push_str(&format!(
            r#"<clipPath id="{id}"><rect x="-10" y="-10" width="40" height="{height}"/></clipPath>"#
        ));
        drawable.push_str(rest);
    }
    drawable
}

#[derive(Clone, PartialEq)]
pub struct LibrarySprite {
    color: Color,
    hovered: bool,
}

impl LibrarySprite {
    pub fn new(color: Color, hovered: bool) -> Self {
        Self { color, hovered }
    }
}

impl Component for LibrarySprite {
    fn render(&self) -> impl IntoElement {
        let play = use_animation_with_dependencies(&self.hovered, |conf, hovered| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to, duration) = if *hovered {
                (0.0, 1.0, motion::LIBRARY_SPRITE_PLAY)
            } else {
                (1.0, 0.0, motion::LIBRARY_SPRITE_REWIND)
            };
            AnimNum::new(from, to).duration(duration).function(Function::Linear)
        });
        // On creation the animation is finished, which for a button that is not hovered is the first frame.
        let progress = play.get().value().clamp(0.0, 1.0);
        let frame = ((progress * FRAMES as f32).floor() as u32).min(FRAMES - 1);

        rect()
            .width(Size::px(FRAME_SIZE))
            .height(Size::px(FRAME_SIZE))
            .overflow(Overflow::Clip)
            // The offset moves a rect's children, so it goes here and not on the icon itself.
            .offset_x(-(frame as f32) * FRAME_SIZE)
            .maybe_child(DRAWABLE_SPRITE.clone().map(|bytes| {
                SvgViewer::new(("zendia-library-sprite", bytes))
                    .width(Size::px(FRAME_SIZE * FRAMES as f32))
                    .height(Size::px(FRAME_SIZE))
                    .color(self.color)
                    .show_loader(false)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_is_rewritten_into_what_the_renderer_draws() {
        let (_, bytes) = prepared_icon_bytes("library-sprite").expect("the sprite icon");
        let svg = drawable_sprite(std::str::from_utf8(&bytes).expect("the sprite is text"));
        assert!(svg.contains(r#"viewBox="0 0 648 18""#));
        assert!(!svg.contains("<mask"));
        assert_eq!(svg.matches("<clipPath ").count(), FRAMES as usize);
        assert!(svg.contains(r#"height="21.635""#));
        assert_eq!(
            svg.matches(r#"class="zen-library-glow"#).count(),
            svg.matches(r#"display="none" class="zen-library-glow"#).count()
        );
    }
}
