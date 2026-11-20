//! fbcon — framebuffer text console over the UEFI Graphics Output Protocol.
//!
//! Mirrors what early fbcon does in Linux: locate the GOP framebuffer, lay a
//! text grid over it using the klib font bitmap (lib/fonts/font_8x16.c), scroll
//! by moving scanlines when the bottom is reached. Only direct-write pixel
//! formats (Rgb/Bgr 32bpp) are supported — that is what QEMU's stdvga/bochs
//! GOP provides; anything else leaves the console absent and serial keeps
//! working.

use core::fmt::{self, Write};
use core::ptr;

use uefi::boot;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

use klib::font::{FONT_8X16, FONT_8X16_H, FONT_8X16_W};

use crate::logo_data::{LOGO_H, LOGO_RGB, LOGO_W};

const FG: u32 = 0x00c0c0c0; // light gray text
const BG: u32 = 0x00000000; // black background
const FG_BOOT: u32 = 0x0000ff7f; // accent for the banner line

pub struct FbConsole {
    base: *mut u8,
    size: usize,
    stride: usize, // pixels per scanline
    width: usize,
    height: usize,
    bgr: bool,
    cols: usize,
    rows: usize,
    top: usize, // first text row; rows above it (the splash) never scroll
    cx: usize,
    cy: usize,
}

impl FbConsole {
    /// Locate the GOP framebuffer and claim it. Returns `None` when no GOP is
    /// present or the active mode is not a direct-write 32bpp format.
    pub fn new() -> Option<Self> {
        let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
        let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;
        let info = gop.current_mode_info();
        let bgr = match info.pixel_format() {
            PixelFormat::Bgr => true,
            PixelFormat::Rgb => false,
            _ => return None, // Bitmask/BltOnly: unsupported for now
        };
        let (width, height) = info.resolution();
        let stride = info.stride();
        let mut fb = gop.frame_buffer();
        let base = fb.as_mut_ptr();
        let size = fb.size();
        if base.is_null() || size < stride * 4 || width < FONT_8X16_W || height < FONT_8X16_H {
            return None;
        }
        let mut con = Self {
            base,
            size,
            stride,
            width,
            height,
            bgr,
            cols: width / FONT_8X16_W,
            rows: height / FONT_8X16_H,
            top: 0,
            cx: 0,
            cy: 0,
        };
        con.clear();
        Some(con)
    }

    #[inline]
    fn px(&mut self, x: usize, y: usize, px: u32) {
        let off = y * self.stride * 4 + x * 4;
        if off + 3 >= self.size {
            return;
        }
        // SAFETY: off+3 is inside the framebuffer per the bounds check above;
        // volatile because this is MMIO-backed memory.
        unsafe {
            let p = self.base.add(off) as *mut u32;
            // Constants are 0x00RRGGBB. Bgr memory order is [B,G,R,X] so the
            // u32 can go out as-is; Rgb wants [R,G,B,X] — swap the R/B lanes.
            let v = if self.bgr {
                px
            } else {
                (px >> 16) | (px & 0x0000_ff00) | ((px & 0xff) << 16)
            };
            ptr::write_volatile(p, v);
        }
    }

    fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.px(x, y, BG);
            }
        }
        self.cx = 0;
        self.cy = 0;
    }

    fn scroll(&mut self) {
        let row_bytes = self.stride * 4;
        let region_top = self.top * FONT_8X16_H * row_bytes;
        let move_bytes = row_bytes * FONT_8X16_H * (self.rows - self.top - 1);
        // SAFETY: src/dst stay inside `size` — the scroll region covers the
        // (rows-top)*GH scanlines starting at top*GH, all below
        // height*stride*4 <= size. rows > top always (splash clamps top).
        unsafe {
            ptr::copy(
                self.base.add(region_top + row_bytes * FONT_8X16_H),
                self.base.add(region_top),
                move_bytes,
            );
        }
        for y in (self.rows - 1) * FONT_8X16_H..self.rows * FONT_8X16_H {
            for x in 0..self.width {
                self.px(x, y, BG);
            }
        }
        self.cy = self.rows - 1;
    }

    fn glyph(&mut self, c: u8) {
        if self.cy >= self.rows {
            self.scroll();
        }
        if self.cx >= self.cols {
            self.newline();
        }
        let gi = c as usize * FONT_8X16_H;
        let x0 = self.cx * FONT_8X16_W;
        let y0 = self.cy * FONT_8X16_H;
        for row in 0..FONT_8X16_H {
            let bits = FONT_8X16[gi + row];
            for col in 0..FONT_8X16_W {
                let v = if bits & (0x80 >> col) != 0 { FG } else { BG };
                self.px(x0 + col, y0 + row, v);
            }
        }
        self.cx += 1;
    }

    fn newline(&mut self) {
        self.cx = 0;
        self.cy += 1;
        if self.cy >= self.rows {
            self.scroll();
        }
    }

    /// Boot splash: draw the generated logo image centered at the top of the
    /// screen, then place the text cursor on the first row below it.
    pub fn splash(&mut self) {
        let lw = LOGO_W.min(self.width);
        let lh = LOGO_H.min(self.height);
        let x0 = (self.width - lw) / 2;
        for y in 0..lh {
            for x in 0..lw {
                let i = (y * LOGO_W + x) * 3;
                let rgb = (u32::from(LOGO_RGB[i]) << 16)
                    | (u32::from(LOGO_RGB[i + 1]) << 8)
                    | u32::from(LOGO_RGB[i + 2]);
                self.px(x0 + x, y, rgb);
            }
        }
        self.cx = 0;
        self.top = lh.div_ceil(FONT_8X16_H).min(self.rows - 1);
        self.cy = self.top;
    }

    /// Accent-colored banner line (proof the console is alive before any
    /// serial-fanned text arrives).
    pub fn banner(&mut self) {
        for (i, &b) in b"rustux: framebuffer console (GOP, font_8x16)".iter().enumerate() {
            if i >= self.cols {
                break;
            }
            let gi = b as usize * FONT_8X16_H;
            let x0 = i * FONT_8X16_W;
            for row in 0..FONT_8X16_H {
                let bits = FONT_8X16[gi + row];
                for col in 0..FONT_8X16_W {
                    let v = if bits & (0x80 >> col) != 0 { FG_BOOT } else { BG };
                    self.px(x0 + col, self.cy * FONT_8X16_H + row, v);
                }
            }
        }
        self.cx = 0;
        self.cy += 1;
    }
}

impl Write for FbConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            match b {
                b'\n' => self.newline(),
                b'\r' => {}
                0x20..=0x7e => self.glyph(b),
                _ => self.glyph(b'?'),
            }
        }
        Ok(())
    }
}
