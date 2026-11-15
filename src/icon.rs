//! Иконка приложения, рисуется кодом: круг + буква «R». Синий — авто вкл, серый — выкл.
pub const SIZE: usize = 32;

pub fn rgba(on: bool) -> Vec<u8> {
    const GLYPH: [&str; 7] = ["1111.", "1...1", "1...1", "1111.", "1.1..", "1..1.", "1...1"];
    let (r, g, b) = if on { (0x2E, 0x7D, 0xF6) } else { (0x88, 0x88, 0x88) };
    let mut px = vec![0u8; SIZE * SIZE * 4];
    let mut put = |x: usize, y: usize, c: (u8, u8, u8)| {
        let i = (y * SIZE + x) * 4;
        px[i..i + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            if dx * dx + dy * dy <= 15.5 * 15.5 {
                put(x, y, (r, g, b));
            }
        }
    }
    for (ry, row) in GLYPH.iter().enumerate() {
        for (rx, ch) in row.chars().enumerate() {
            if ch == '1' {
                for sy in 0..3 {
                    for sx in 0..3 {
                        put(8 + rx * 3 + sx, 5 + ry * 3 + sy, (255, 255, 255));
                    }
                }
            }
        }
    }
    px
}
