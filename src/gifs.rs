use egui::Context;
use std::collections::HashMap;

const DEFAULT_SPEED: f32 = 1.5;

pub struct Gifs {
    map: HashMap<&'static str, &'static [u8]>,
}

impl Gifs {
    pub fn new() -> Self {
        let mut map: HashMap<&'static str, &'static [u8]> = HashMap::new();
        map.insert("council", include_bytes!("../gifs/council.gif"));
        map.insert("councillor", include_bytes!("../gifs/council.gif"));
        map.insert("designer", include_bytes!("../gifs/designer.gif"));
        map.insert("explorer", include_bytes!("../gifs/explorer.gif"));
        map.insert("fixer", include_bytes!("../gifs/fixer.gif"));
        map.insert("input", include_bytes!("../gifs/question.gif"));
        map.insert("intro", include_bytes!("../gifs/intro.gif"));
        map.insert("librarian", include_bytes!("../gifs/librarian.gif"));
        map.insert("oracle", include_bytes!("../gifs/oracle.gif"));
        map.insert("orchestrator", include_bytes!("../gifs/orchestrator.gif"));
        Self { map }
    }

    pub fn register(&self, ctx: &Context, speed: f32) {
        egui_extras::install_image_loaders(ctx);
        let speed = normalized_speed(speed);
        for (name, bytes) in &self.map {
            ctx.include_bytes(
                format!("bytes://{name}@{speed:.2}.gif"),
                speed_gif(bytes, speed),
            );
        }
    }

    pub fn uri(&self, agent: &str, gif_pack: &str, speed: f32) -> String {
        let name = if gif_pack == "default" && self.map.contains_key(agent) {
            agent
        } else {
            "orchestrator"
        };
        format!("bytes://{name}@{:.2}.gif", normalized_speed(speed))
    }
}

pub fn normalized_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(0.25, 4.0)
    } else {
        DEFAULT_SPEED
    }
}

fn speed_gif(bytes: &[u8], speed: f32) -> Vec<u8> {
    let speed = normalized_speed(speed);
    if (speed - 1.0).abs() < f32::EPSILON {
        return bytes.to_vec();
    }

    let mut out = bytes.to_vec();
    let mut i = 0;
    while i + 7 < out.len() {
        // GIF Graphic Control Extension:
        // 21 F9 04 <packed> <delay lo> <delay hi> <transparent index> 00
        if out[i] == 0x21 && out[i + 1] == 0xF9 && out[i + 2] == 0x04 {
            let delay = u16::from_le_bytes([out[i + 4], out[i + 5]]);
            if delay > 0 {
                let scaled = ((delay as f32) / speed).round().clamp(1.0, u16::MAX as f32) as u16;
                let [lo, hi] = scaled.to_le_bytes();
                out[i + 4] = lo;
                out[i + 5] = hi;
            }
            i += 8;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::speed_gif;

    #[test]
    fn speed_gif_scales_graphic_control_extension_delay() {
        let gif = [
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x21, 0xF9, 0x04, 0x00, 10, 0, 0x00, 0x00,
        ];

        let sped_up = speed_gif(&gif, 2.0);
        assert_eq!(u16::from_le_bytes([sped_up[10], sped_up[11]]), 5);

        let slowed_down = speed_gif(&gif, 0.5);
        assert_eq!(u16::from_le_bytes([slowed_down[10], slowed_down[11]]), 20);
    }
}
