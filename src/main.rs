#![allow(unused_variables)]

use ascii::AsciiString;
use bytes::{Buf, BufMut, BytesMut};
use enumflags2::BitFlags;
use std::io;
use std::time::Duration;
use std::vec::Vec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::delay_for;

#[repr(u8)]
enum Opcodes {
    CreateWindow = 1,
    MapWindow = 8,
}

enum ImageByteOrder {
    LSBFirst,
    MSBFirst,
}

enum BitmapFormatBitOrder {
    LeastSignificant,
    MostSignificant,
}

#[derive(Debug)]
enum BackingStore {
    Never,
    WhenMapped,
    Always,
}

#[derive(Debug)]
enum Class {
    StaticGray,
    GrayScale,
    StaticColor,
    PseudoColor,
    TrueColor,
    DirectColor,
}

#[derive(BitFlags, Copy, Clone, Debug)]
#[repr(u32)]
pub enum Event {
    KeyPress = 0x00000001,
    KeyRelease = 0x00000002,
    ButtonPress = 0x00000004,
    ButtonRelease = 0x00000008,
    EnterWindow = 0x00000010,
    LeaveWindow = 0x00000020,
    PointerMotion = 0x00000040,
    PointerMotionHint = 0x00000080,
    Button1Motion = 0x00000100,
    Button2Motion = 0x00000200,
    Button3Motion = 0x00000400,
    Button4Motion = 0x00000800,
    Button5Motion = 0x00001000,
    ButtonMotion = 0x00002000,
    KeymapState = 0x00004000,
    Exposure = 0x00008000,
    VisibilityChange = 0x00010000,
    StructureNotify = 0x00020000,
    ResizeRedirect = 0x00040000,
    SubstructureNotify = 0x00080000,
    SubstructureRedirect = 0x00100000,
    FocusChange = 0x00200000,
    PropertyChange = 0x00400000,
    ColormapChange = 0x00800000,
    OwnerGrabButton = 0x01000000,
}

type WindowId = u32;
type ColorMap = u32;
type VisualId = u32;

#[derive(Debug)]
struct Format {
    depth: u8,
    bits_per_pixel: u8,
    scanline_pad: u8,
}

#[derive(Debug)]
struct Screen {
    window: WindowId,
    default_colormap: ColorMap,
    white_pixel: u32,
    black_pixel: u32,
    current_input_masks: BitFlags<Event>,
    width_pixels: u16,  // in pixels
    height_pixels: u16, // in pixels
    width_mm: u16,      // in millimeters
    height_mm: u16,     // in millimeters
    min_installed_maps: u16,
    max_installed_maps: u16,
    root_visual: VisualId,
    backing_stores: BackingStore,
    save_unders: bool,
    root_depth: u8,
    number_depths_in_allowed_depths: u8,
    allowed_depths: Vec<Depth>,
}

#[derive(Debug)]
struct Depth {
    depth: u8,
    number_visual_types: u16,
    visuals: Vec<VisualType>,
}

#[derive(Debug)]
struct VisualType {
    visual_id: VisualId,
    class: Class,
    bits_per_rgb_value: u8,
    colormap_entries: u16,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
}

fn create_window_request(buf: &mut BytesMut) -> WindowId {
    buf.put_u8(Opcodes::CreateWindow as u8); // opcode
    buf.put_u8(0); // depth, 0 means copy from parent
    buf.put_u16_le(8 /* + values.len() */); // request len
    buf.put_u32_le(0); // wid
    buf.put_u32_le(0); // parent
    buf.put_i16_le(100); // x
    buf.put_i16_le(100); // y
    buf.put_u16_le(0); // width
    buf.put_u16_le(0); // height
    buf.put_u16_le(1); // border-width
    buf.put_u16_le(1); // class InputOutput
    buf.put_u32_le(33); // visual id
    buf.put_u32_le(0); // bitmask

    0u32
}

// pad(E) = (4 - (E mod 4)) mod 4
fn pad(len: usize) -> usize {
    (4 - (len % 4)) % 4
}

fn map_window_request(buf: &mut BytesMut, window_id: WindowId) {
    buf.put_u8(Opcodes::MapWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(0); // request length
    buf.put_u32_le(window_id);
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = UnixStream::connect("/tmp/.X11-unix/X1").await?; // Xnest server
    let mut connection_req = BytesMut::with_capacity(12);
    connection_req.put_u8(0x6c); // little endian byte order (LSB first)
    connection_req.put_u8(0); // unused
    connection_req.put_u16_le(11); // protocol major version
    connection_req.put_u16_le(0); // protocol minor version
    connection_req.put_u16_le(0); // length of authorization-protocol-name
    connection_req.put_u16_le(0); // length of authorization-protocol-data
    connection_req.put_u16_le(0);
    stream.write_all(connection_req.bytes()).await?;
    eprintln!("write");

    // let mut connection_repl = BytesMut::with_capacity(12);
    // let n = stream.read(&mut connection_repl).await?;
    // eprintln!("{} - {:?}", n, connection_repl);

    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await?;
    eprintln!("{} - {:?}", n, &buf[..n]);
    let mut response = &buf[..n];
    let status_code = response.get_u8();
    match status_code {
        0 => panic!("failed"),
        1 => eprintln!("success"),
        2 => eprintln!("authenticate"),
        x => panic!("unknown response status code: {}", x),
    }

    response.advance(1); // unused pad

    let protocol_major_version = response.get_u16_le();
    let protocol_minor_version = response.get_u16_le();

    eprintln!(
        "version major: {}, minor: {}",
        protocol_major_version, protocol_minor_version
    );

    let additional_data_len = response.get_u16_le();
    eprintln!("additional data len: {} [bytes]", additional_data_len * 4);

    let release_number = response.get_u32_le();
    let resource_id_base = response.get_u32_le();
    let resource_id_mask = response.get_u32_le();
    let motion_buffer_size = response.get_u32_le();
    let vendor_len = response.get_u16_le() as usize;
    let maximum_request_length = response.get_u16_le();
    let number_screens_roots = response.get_u8() as usize;
    let number_formats = response.get_u8() as usize;

    eprintln!(
        "number of screens: {}, number of formats: {}",
        number_screens_roots, number_formats
    );

    let image_byte_order = match response.get_u8() {
        0 => ImageByteOrder::LSBFirst,
        1 => ImageByteOrder::MSBFirst,
        x => panic!("unknown image byte order {}", x),
    };

    let bitmap_format_bit_order = match response.get_u8() {
        0 => BitmapFormatBitOrder::LeastSignificant,
        1 => BitmapFormatBitOrder::MostSignificant,
        x => panic!("unknown bitmap format bit order {}", x),
    };

    let bitmap_format_scanline_unit = response.get_u8();
    let bitmap_format_scanline_pad = response.get_u8();

    let min_keycode = response.get_u8();
    let max_keycode = response.get_u8();

    response.advance(4);

    eprintln!(
        "{}",
        AsciiString::from_ascii(&response[..vendor_len]).expect("must be ASCII")
    );
    response.advance(vendor_len + pad(vendor_len));

    let mut formats: Vec<Format> = Vec::new();
    for _current_format in 0..number_formats {
        formats.push(Format {
            depth: response.get_u8(),
            bits_per_pixel: response.get_u8(),
            scanline_pad: response.get_u8(),
        });
        response.advance(5);
    }

    let mut screen_roots: Vec<Screen> = Vec::with_capacity(number_screens_roots);
    for i in 0..number_screens_roots {
        screen_roots.push(Screen {
            window: response.get_u32_le(),
            default_colormap: response.get_u32_le(),
            white_pixel: response.get_u32_le(),
            black_pixel: response.get_u32_le(),
            current_input_masks: BitFlags::from_bits(response.get_u32_le())
                .expect("valid input masks"),
            width_pixels: response.get_u16_le(),
            height_pixels: response.get_u16_le(),
            width_mm: response.get_u16_le(),
            height_mm: response.get_u16_le(),
            min_installed_maps: response.get_u16_le(),
            max_installed_maps: response.get_u16_le(),
            root_visual: response.get_u32_le(),
            backing_stores: match response.get_u8() {
                0 => BackingStore::Never,
                1 => BackingStore::WhenMapped,
                2 => BackingStore::Always,
                other => panic!("unknown backing store code {}", other),
            },
            save_unders: match response.get_u8() {
                0 => false,
                1 => true,
                other => panic!("save unders must be either 0 or 1, but is {}", other),
            },
            root_depth: response.get_u8(),
            number_depths_in_allowed_depths: response.get_u8(),
            allowed_depths: Vec::new(),
        });
        let last_screen = &mut screen_roots.last_mut().unwrap();
        for _allowed_depth in 0..(last_screen.number_depths_in_allowed_depths) {
            last_screen.allowed_depths.push(Depth {
                depth: {
                    let depth = response.get_u8();
                    response.advance(1);
                    depth
                },
                number_visual_types: {
                    let number_of_visual_types = response.get_u16_le();
                    response.advance(4);
                    number_of_visual_types
                },
                visuals: Vec::new(),
            });

            let last_allowed_depth = &mut last_screen.allowed_depths.last_mut().unwrap();
            for _visual in 0..(last_allowed_depth.number_visual_types) {
                last_allowed_depth.visuals.push(VisualType {
                    visual_id: response.get_u32_le(),
                    class: match response.get_u8() {
                        0 => Class::StaticGray,
                        1 => Class::GrayScale,
                        2 => Class::StaticColor,
                        3 => Class::PseudoColor,
                        4 => Class::TrueColor,
                        5 => Class::DirectColor,
                        other => panic!("unknown visual class {}", other),
                    },
                    bits_per_rgb_value: response.get_u8(),
                    colormap_entries: response.get_u16_le(),
                    red_mask: response.get_u32_le(),
                    green_mask: response.get_u32_le(),
                    blue_mask: response.get_u32_le(),
                });
                response.advance(4);
            }
        }
    }

    eprintln!("{:?}", screen_roots.first().unwrap());
    eprintln!(
        "remaining from response: {} {} {}",
        response.remaining(),
        additional_data_len * 4,
        n
    );

    let mut request_buf = BytesMut::new();
    let window_id = create_window_request(&mut request_buf);
    stream.write_all(request_buf.bytes()).await?;

    let n = stream.read(&mut buf).await?;
    eprintln!("{} - {:?}", n, &buf[..n]);

    let mut request_buf = BytesMut::new();
    map_window_request(&mut request_buf, window_id);

    delay_for(Duration::from_secs(10)).await;

    Ok(())
}
