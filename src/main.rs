#![warn(rust_2018_idioms)]
#![allow(unused_variables)]

use ascii::AsciiString;
use bytes::{Buf, BufMut, BytesMut};
use colored::Colorize;
use enumflags2::{bitflags, make_bitflags, BitFlags};
use num_traits::FromPrimitive;
use std::convert::TryInto;
use std::io;
use std::iter::Iterator;
use std::time::Duration;
use std::vec::Vec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

#[derive(Debug, num_derive::FromPrimitive)]
#[repr(u8)]
enum Opcodes {
    CreateWindow = 1,
    ChangeWindowAttributes = 2,
    GetWindowAttributes = 3,
    DestroyWindow = 4,
    MapWindow = 8,
    MapSubwindows = 9,
    UnmapWindow = 10,
    UnmapSubwindows = 11,
    ConfigureWindow = 12,
    CirculateWindow = 13,
    GetGeometry = 14,
    QueryTree = 15,
    CreateGC = 55,
    ChangeGC = 56,
    CopyGC = 57,
    FreeGC = 60,
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

#[derive(Debug, num_derive::FromPrimitive)]
#[repr(u8)]
enum ErrorCode {
    Request = 1,
    Value = 2,
    Window = 3,
    Pixmap = 4,
    Atom = 5,
    Cursor = 6,
    Font = 7,
    Match = 8,
    Drawable = 9,
    Access = 10,
    Alloc = 11,
    Colormap = 12,
    GContext = 13,
    IDChoice = 14,
    Name = 15,
    Length = 16,
    Implementation = 17,
}

#[bitflags]
#[derive(Copy, Clone, Debug)]
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

#[derive(Debug, num_derive::FromPrimitive)]
#[repr(u8)]
enum Events {
    KeyPress = 2,
    KeyRelease = 3,
    ButtonPress = 4,
    ButtonRelease = 5,
    MotionNotify = 6,
    EnterNotify = 7,
    LeaveNotify = 8,
    FocusIn = 9,
    FocusOut = 10,
    KeymapNotify = 11,
    Expose = 12,
    GraphicsExposure = 13,
    NoExposure = 14,
    VisibilityNotify = 15,
    CreateNotify = 16,
    DestroyNotify = 17,
    UnmapNotify = 18,
    MapNotify = 19,
    MapRequest = 20,
    // ...
    SelectionRequest = 30,
    SelectionNotify = 31,
    ColormapNotify = 32,
    ClientMessage = 33,
    MappingNotify = 34,
}

type WindowId = u32;
type GCId = u32;
type ColorMap = u32;
type VisualId = u32;

#[derive(Debug)]
struct Error {}

#[derive(Debug)]
struct Format {
    depth: u8,
    bits_per_pixel: u8,
    scanline_pad: u8,
}

#[derive(Debug)]
struct Connection {
    resource_id_base: u32,
    resource_id_mask: u32,
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

fn create_window_request(
    buf: &mut impl BufMut,
    connection: &Connection,
    screen: &Screen,
    id_generator: &mut impl Iterator<Item = u32>,
) -> WindowId {
    #[repr(u32)]
    enum BitmaskValues {
        BackgroundPixmap = 0x00000001,
        BackgroundPixel = 0x00000002,
        BorderPixmap = 0x00000004,
        BorderPixel = 0x00000008,
        BitGravity = 0x00000010,
        WinGravity = 0x00000020,
        BackingStore = 0x00000040,
        BackingPlanes = 0x00000080,
        BackingPixel = 0x00000100,
        OverrideRedirect = 0x00000200,
        SaveUnder = 0x00000400,
        EventMask = 0x00000800,
        DoNotPropagateMask = 0x00001000,
        Colormap = 0x00002000,
        Cursor = 0x00004000,
    }

    buf.put_u8(Opcodes::CreateWindow as u8); // opcode
    buf.put_u8(0); // depth, 0 means copy from parent
    buf.put_u16_le(8 + 2 /* + values.len() */); // request len
    let id = if let Some(id) = id_generator.next() {
        buf.put_u32_le(id); // wid
        id
    } else {
        panic!("no more ids");
    };
    buf.put_u32_le(screen.window); // parent
    buf.put_i16_le(200); // x
    buf.put_i16_le(200); // y
    buf.put_u16_le(100); // width
    buf.put_u16_le(100); // height
    buf.put_u16_le(4); // border-width
    buf.put_u16_le(0); // class InputOutput
    buf.put_u32_le(screen.root_visual); // visual id
    buf.put_u32_le(BitmaskValues::BackgroundPixel as u32 | BitmaskValues::EventMask as u32); // bitmask

    // list-of-values
    //
    // values must be given in the order defined by the value of
    // BitmaskValues, for example:
    //
    // the value for BitmaskValues::BorderPixmap must be defined
    // before BitmaskValues::EventMask
    buf.put_u32_le(screen.white_pixel); // background-pixel
    buf.put_u32_le(
        make_bitflags!(Event::{
            KeyPress |
            KeyRelease |
            ButtonPress |
            ButtonRelease |
            EnterWindow |
            LeaveWindow})
        .bits(),
    ); // event-mask

    id
}

fn destroy_window_request(buf: &mut impl BufMut, wid: WindowId) {
    buf.put_u8(Opcodes::DestroyWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(wid); // wid
}

fn get_window_attributes_request(buf: &mut impl BufMut, wid: WindowId) {
    buf.put_u8(Opcodes::GetWindowAttributes as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(wid); // wid
}

// pad(E) = (4 - (E mod 4)) mod 4
const fn pad(len: usize) -> usize {
    (4 - (len % 4)) % 4
}

fn map_window_request(buf: &mut impl BufMut, window_id: WindowId) {
    buf.put_u8(Opcodes::MapWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(window_id);
}

fn unmap_window_request(buf: &mut impl BufMut, window_id: WindowId) {
    buf.put_u8(Opcodes::UnmapWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(window_id);
}

#[repr(u8)]
enum StackModes {
    Above = 0,
    Below = 1,
    TopIf = 2,
    BottomIf = 3,
    Opposite = 4,
}

enum ConfigureWindowCommands {
    X(i16),
    Y(i16),
    Width(u16),
    Height(u16),
    BorderWidth(u16),
    Sibling(WindowId),
    StackMode(StackModes),
}

fn configure_window(
    buf: &mut impl BufMut,
    window_id: WindowId,
    commands: &[ConfigureWindowCommands],
) {
    #[repr(u16)]
    enum BitmaskValues {
        X = 0x0001,
        Y = 0x0002,
        Width = 0x0004,
        Height = 0x0008,
        BorderWidth = 0x0010,
        Sibling = 0x0020,
        StackMode = 0x0040,
    }
    let n = commands.len();
    buf.put_u8(Opcodes::UnmapWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le((3 + n).try_into().unwrap()); // request length
    buf.put_u32_le(window_id);
    buf.put_u16_le(BitmaskValues::X as u16); // value-mask
    buf.put_u16_le(0); // unused

    buf.put_i16_le(5); // x value
    buf.put_u16_le(0); // padding
}

fn create_gc(
    buf: &mut impl BufMut,
    connection: &Connection,
    window_id: WindowId,
    id_generator: &mut impl Iterator<Item = u32>,
) -> GCId {
    buf.put_u8(Opcodes::CreateGC as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(4 + 0); // request length
    let id = if let Some(id) = id_generator.next() {
        buf.put_u32_le(id); // cid
        id
    } else {
        panic!("no more ids");
    };
    buf.put_u32_le(window_id); // drawable
    buf.put_u32_le(0); // bitmask

    // values list

    id
}

fn free_gc(buf: &mut impl BufMut, gc_id: GCId) {
    buf.put_u8(Opcodes::FreeGC as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(gc_id);
}

fn decode_event(event: Events, buf: &mut impl Buf) {
    if buf.remaining() < 31 {
        return;
    }

    eprintln!("event: {:?}", event);

    match event {
        Events::KeyPress | Events::KeyRelease => {
            let detail = buf.get_u8(); // keycode
            let sequence_number = buf.get_u16_le();
            let timestamp = buf.get_u32_le();
            // 1     KEYCODE                         detail
            // 2     CARD16                          sequence number
            // 4     TIMESTAMP                       time
            // 4     WINDOW                          root
            // 4     WINDOW                          event
            // 4     WINDOW                          child
            // 0     None
            // 2     INT16                           root-x
            // 2     INT16                           root-y
            // 2     INT16                           event-x
            // 2     INT16                           event-y
            // 2     SETofKEYBUTMASK                 state
            // 1     BOOL                            same-screen
            // 1                                     unused
            buf.advance(24);

            eprintln!("keycode: {}", detail);
        }
        Events::ButtonPress | Events::ButtonRelease => {
            let detail = buf.get_u8(); // keycode
            let sequence_number = buf.get_u16_le();
            let timestamp = buf.get_u32_le();

            buf.advance(24);

            eprintln!("button: {}", detail);
        }
        Events::EnterNotify | Events::LeaveNotify => {
            let detail = buf.get_u8();
            let sequence_number = buf.get_u16_le();
            let timestamp = buf.get_u32_le();
            let root_window = buf.get_u32_le();
            let event_window = buf.get_u32_le();
            let child_window = buf.get_u32_le();
            let (root_x, root_y) = (buf.get_u16_le(), buf.get_u16_le());
            let (event_x, event_y) = (buf.get_u16_le(), buf.get_u16_le());
            let state = buf.get_u16_le();
            let mode = buf.get_u8();
            let same_screen_focus = buf.get_u8();
        }
        Events::MappingNotify => {
            buf.advance(1); // unused
            let sequence_number = buf.get_u16_le();
            let request = buf.get_u8();
            let key_code = buf.get_u8();
            let count = buf.get_u8();
            eprintln!(
                "sequence_number: {}, request: {}, key_code: {}, count: {}",
                sequence_number, request, key_code, count
            );
            buf.advance(25); // unused
        }
        _ => panic!("unable to decode event yet: {:?}", event),
    }
}

struct IdGenerator {
    last: u32,
    max: u32,
    base: u32,
    inc: u32,
}

impl IdGenerator {
    fn new(base: u32, mask: u32) -> Self {
        eprintln!("inc {}", mask & (!mask + 1));
        Self {
            last: 0,
            max: mask,
            base: base,
            inc: mask & (!mask + 1),
        }
    }
}

impl Iterator for IdGenerator {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        // naive implementation for now
        //
        // it should handle overflows in the future and should return
        // None in this case
        self.last += self.inc;

        Some(self.last | self.base)
    }
}

#[tokio::main(flavor = "current_thread")]
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
    stream.write_all_buf(&mut connection_req).await?;

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
        1 => eprintln!("{}", "success".green()),
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
    let connection = Connection {
        resource_id_base,
        resource_id_mask,
    };
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

    let screen = screen_roots.first().unwrap();
    eprintln!("{:?}", screen);
    eprintln!(
        "remaining from response: {} {} {}",
        response.remaining(),
        additional_data_len * 4,
        n
    );

    let (mut read_stream, write_stream) = stream.into_split();
    let (tx, mut rx): (
        tokio::sync::mpsc::Sender<(Opcodes, oneshot::Sender<i32>)>,
        tokio::sync::mpsc::Receiver<(Opcodes, oneshot::Sender<i32>)>,
    ) = mpsc::channel(1);

    let mut stream = write_stream;
    tokio::spawn(async move {
        loop {
            // Every reply contains a 32-bit length field expressed in units
            // of four bytes. Every reply consists of 32 bytes followed by
            // zero or more additional bytes of data, as specified in the
            // length field. Unused bytes within a reply are not guaranteed to
            // be zero. Every reply also contains the least significant 16
            // bits of the sequence number of the corresponding request.
            let mut response_buf = BytesMut::new();
            let n = read_stream.read_buf(&mut response_buf).await;
            while response_buf.remaining() >= 32 {
                let first_byte = response_buf.get_u8();

                if first_byte == 0 {
                    // Error
                    let raw_error_code = response_buf.get_u8();
                    eprintln!("raw_error_code: {}", raw_error_code);
                    let error_code = ErrorCode::from_u8(raw_error_code).expect("valid error code");
                    eprintln!("code field: {:?}", error_code);
                    eprintln!("sequence number: {}", response_buf.get_u16_le());
                    match error_code {
                        ErrorCode::IDChoice | ErrorCode::Window => {
                            eprintln!("bad resource id: {}", response_buf.get_u32_le());
                        }
                        ErrorCode::Request | ErrorCode::Match | ErrorCode::Length => {
                            response_buf.advance(4); // unused
                        }
                        _ => unimplemented!("error code not implemented {:?}", error_code),
                    }
                    eprintln!("minor opcode: {}", response_buf.get_u16_le());
                    let major_opcode = response_buf.get_u8();
                    eprintln!(
                        "major opcode: {} {:?}",
                        major_opcode,
                        Opcodes::from_u8(major_opcode)
                    );
                    response_buf.advance(21); // 21 unused bytes
                    eprintln!("--");
                } else if first_byte == 1 {
                    // Reply
                    let reply_info = rx.recv().await;
                    if let Some((opcode, one_tx)) = reply_info {
                        eprintln!("received reply: {:?}, opcode: {:?}", response_buf, opcode);
                        match opcode {
                            Opcodes::GetWindowAttributes => {
                                let backing_store = response_buf.get_u8();
                                let sequence_number = response_buf.get_u16_le();
                                let reply_length = response_buf.get_u32_le();
                                eprintln!("reply_length: {}", reply_length);
                                response_buf.advance(36);
                            }
                            _ => panic!("unknown opcode {:?}", opcode),
                        }
                        one_tx.send(23);
                    }
                } else if let Some(event) = Events::from_u8(first_byte) {
                    decode_event(event, &mut response_buf);
                }
            }
        }
    });

    let mut id_generator = IdGenerator::new(resource_id_base, resource_id_mask);

    let mut request_buf = BytesMut::new();
    let window_id =
        create_window_request(&mut request_buf, &connection, &screen, &mut id_generator);
    stream.write_all_buf(&mut request_buf).await?;

    let mut request_buf = BytesMut::new();
    map_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    let mut request_buf = BytesMut::new();
    get_window_attributes_request(&mut request_buf, window_id);
    let (one_tx, mut one_rx) = oneshot::channel();
    tx.send((Opcodes::GetWindowAttributes, one_tx)).await;
    stream.write_all_buf(&mut request_buf).await?;
    let x: i32 = one_rx.await.unwrap();
    eprintln!("wrote get_window_attributes_request {}", x);

    let gc_id = create_gc(&mut request_buf, &connection, window_id, &mut id_generator);
    stream.write_all_buf(&mut request_buf).await?;

    sleep(Duration::from_secs(10)).await;

    free_gc(&mut request_buf, gc_id);
    stream.write_all_buf(&mut request_buf).await?;

    let mut request_buf = BytesMut::new();
    unmap_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    let mut request_buf = BytesMut::new();
    destroy_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    Ok(())
}
