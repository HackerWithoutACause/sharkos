use bootloader_api::info::FrameBuffer;
use core::fmt::Write;
use lazy_static::lazy_static;
use spinning_top::Spinlock;
use volatile::Volatile;

lazy_static! {
    pub static ref WRITER: Spinlock<Option<Writer>> = Spinlock::new(None);
}

pub fn initialize(framebuffer: &'static mut FrameBuffer) {
    WRITER.lock().replace(Writer::new(framebuffer));
}

pub struct Writer {
    width: usize,
    height: usize,
    stride: usize,
    cursor: usize,
    line: usize,
    pub buffer: &'static mut [u8],
}

impl Writer {
    fn new(framebuffer: &'static mut FrameBuffer) -> Self {
        framebuffer.buffer_mut().fill(0);

        Writer {
            width: framebuffer.info().width,
            height: framebuffer.info().height,
            stride: framebuffer.info().stride,
            cursor: 0,
            line: 0,
            buffer: framebuffer.buffer_mut(),
        }
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}

use noto_sans_mono_bitmap::{get_raster, FontWeight, RasterHeight};

impl core::fmt::Write for Writer {
    fn write_str(&mut self, message: &str) -> core::fmt::Result {
        for c in message.chars() {
            if c == '\n' {
                self.line += 1;
                self.cursor = 0;
                continue;
            }

            let raster = get_raster(c, FontWeight::Regular, RasterHeight::Size20).unwrap();

            if self.line > 25 {
                self.line = 0;
            }

            if self.cursor + raster.width() > self.width - 500 {
                self.cursor = 0;
                self.line += 1;
            }

            for x in 0..raster.width() {
                for y in 0..raster.height() {
                    for z in 0..4 {
                        self.buffer[(x + 50 + self.cursor) * 4
                            + (y + 50 + self.line * raster.height()) * self.stride * 4
                            + z] = raster.raster()[y][x];
                    }
                }
            }

            self.cursor += raster.width();
        }

        Ok(())
    }
}

// impl Writer {
//     pub fn write_byte(&mut self, byte: u8) {
//         match byte {
//             b'\n' => self.new_line(),
//             byte => {
//                 if self.column_position >= BUFFER_WIDTH {
//                     self.new_line();
//                 }

//                 let row = BUFFER_HEIGHT - 1;
//                 let col = self.column_position;

//                 let color_code = self.color_code;
//                 self.buffer.chars[row][col].write(ScreenChar {
//                     ascii_character: byte,
//                     color_code,
//                 });
//                 self.column_position += 1;
//             }
//         }
//     }

//     fn new_line(&mut self) {
//         for row in 1..BUFFER_HEIGHT {
//             for col in 0..BUFFER_WIDTH {
//                 let character = self.buffer.chars[row][col].read();
//                 self.buffer.chars[row - 1][col].write(character);
//             }
//         }
//         self.clear_row(BUFFER_HEIGHT - 1);
//         self.column_position = 0;
//     }

//     fn clear_row(&mut self, row: usize) {
//         let blank = ScreenChar {
//             ascii_character: b' ',
//             color_code: self.color_code,
//         };
//         for col in 0..BUFFER_WIDTH {
//             self.buffer.chars[row][col].write(blank);
//         }
//     }
// }
