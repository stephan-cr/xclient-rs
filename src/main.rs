#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![allow(unused_variables)]
#![allow(dead_code)]

use ascii::AsciiString;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::{crate_name, crate_version, value_parser, Arg, Command};
use colored::Colorize;
use enumflags2::{bitflags, make_bitflags, BitFlags};
use num_traits::FromPrimitive;
use std::convert::TryInto;
use std::error;
use std::iter::Iterator;
use std::string::ToString;
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
    SetInputFocus = 42,
    GetInputFocus = 43,
    QueryKeymap = 44,
    OpenFont = 45,
    CloseFont = 46,
    QueryFont = 47,
    ListFonts = 49,
    ListFontsWithInfo = 50,
    CreatePixmap = 53,
    FreePixmap = 54,
    CreateGC = 55,
    ChangeGC = 56,
    CopyGC = 57,
    FreeGC = 60,
    ImageText8 = 76,
    ImageText16 = 77,
    QueryExtension = 98,
    ListExtensions = 99,
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
    KeyPress = 0x0000_0001,
    KeyRelease = 0x0000_0002,
    ButtonPress = 0x0000_0004,
    ButtonRelease = 0x0000_0008,
    EnterWindow = 0x0000_0010,
    LeaveWindow = 0x0000_0020,
    PointerMotion = 0x0000_0040,
    PointerMotionHint = 0x0000_0080,
    Button1Motion = 0x0000_0100,
    Button2Motion = 0x0000_0200,
    Button3Motion = 0x0000_0400,
    Button4Motion = 0x0000_0800,
    Button5Motion = 0x0000_1000,
    ButtonMotion = 0x0000_2000,
    KeymapState = 0x0000_4000,
    Exposure = 0x0000_8000,
    VisibilityChange = 0x0001_0000,
    StructureNotify = 0x0002_0000,
    ResizeRedirect = 0x0004_0000,
    SubstructureNotify = 0x0008_0000,
    SubstructureRedirect = 0x0010_0000,
    FocusChange = 0x0020_0000,
    PropertyChange = 0x0040_0000,
    ColormapChange = 0x0080_0000,
    OwnerGrabButton = 0x0100_0000,
}

#[derive(Copy, Clone, Debug, num_derive::FromPrimitive)]
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

#[derive(Copy, Clone, Debug, num_derive::FromPrimitive)]
#[repr(u8)]
enum MappingNotifyRequest {
    Modifier = 0,
    Keyboard = 1,
    Pointer = 2,
}

#[bitflags]
#[derive(Copy, Clone, Debug)]
#[repr(u32)]
enum CreateGcBits {
    Function = 0x1,
    PlaneMask = 0x2,
    Foreground = 0x4,
    Background = 0x8,
    LineWidth = 0x10,
    LineStyle = 0x20,
    CapStyle = 0x40,
    JoinStyle = 0x80,
    FillStyle = 0x100,
    FillRule = 0x200,
    Tile = 0x400,
    Stipple = 0x800,
    TileStippleXOrigin = 0x1000,
    TileStippleYOrigin = 0x2000,
    Font = 0x4000,
    SubwindowMode = 0x8000,
    GraphicsExposures = 0x10000,
    ClipXOrigin = 0x20000,
    ClipYOrigin = 0x40000,
    ClipMask = 0x80000,
    DashOffset = 0x100000,
    Dashes = 0x200000,
    ArcMode = 0x400000,
}

type WindowId = u32;
type GCId = u32;
type ColorMap = u32;
type PixmapId = u32;
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
        BackgroundPixmap = 0x0000_0001,
        BackgroundPixel = 0x0000_0002,
        BorderPixmap = 0x0000_0004,
        BorderPixel = 0x0000_0008,
        BitGravity = 0x0000_0010,
        WinGravity = 0x0000_0020,
        BackingStore = 0x0000_0040,
        BackingPlanes = 0x0000_0080,
        BackingPixel = 0x0000_0100,
        OverrideRedirect = 0x0000_0200,
        SaveUnder = 0x0000_0400,
        EventMask = 0x0000_0800,
        DoNotPropagateMask = 0x0000_1000,
        Colormap = 0x0000_2000,
        Cursor = 0x0000_4000,
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
            LeaveWindow |
            Exposure})
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

#[derive(Debug)]
struct WindowAttributesReply {
    backing_store: u8,
    sequence_number: u16,
    reply_length: u32,
}

impl WindowAttributesReply {
    fn from_bytes(buf: &mut impl Buf) -> Self {
        let this = Self {
            backing_store: buf.get_u8(),
            sequence_number: buf.get_u16_le(),
            reply_length: buf.get_u32_le(),
        };
        buf.advance(36);

        this
    }
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
    x: i16,
    y: i16,
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
    buf.put_u8(Opcodes::ConfigureWindow as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le((3 + n).try_into().unwrap()); // request length
    buf.put_u32_le(window_id);
    buf.put_u16_le(BitmaskValues::X as u16 | BitmaskValues::Y as u16); // value-mask
    buf.put_u16_le(0); // unused

    buf.put_i16_le(200 + x); // x value
    buf.put_u16_le(0); // padding
    buf.put_i16_le(200 + y);
    buf.put_u16_le(0); // padding
}

fn create_gc(
    buf: &mut impl BufMut,
    connection: &Connection,
    window_id: WindowId,
    font_id: u32,
    id_generator: &mut impl Iterator<Item = u32>,
) -> GCId {
    let number_of_flags_in_bitmask = 3;
    buf.put_u8(Opcodes::CreateGC as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(4 + number_of_flags_in_bitmask); // request length
    let id = if let Some(id) = id_generator.next() {
        buf.put_u32_le(id); // cid
        id
    } else {
        panic!("no more ids");
    };
    buf.put_u32_le(window_id); // drawable
    buf.put_u32_le(
        CreateGcBits::Foreground as u32
            | CreateGcBits::Background as u32
            | CreateGcBits::Font as u32,
    ); // bitmask

    // values list
    buf.put_u32_le(0xFF00FF00); // foreground
    buf.put_u32_le(0xFF000000); // background
    buf.put_u32_le(font_id); // font id

    id
}

fn free_gc(buf: &mut impl BufMut, gc_id: GCId) {
    buf.put_u8(Opcodes::FreeGC as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(2); // request length
    buf.put_u32_le(gc_id);
}

fn list_fonts(buf: &mut impl BufMut) -> () {
    let pattern_length: u16 = 1;
    let pad = pad(pattern_length as usize) as u16;
    let request_length: u16 = 2 + (pattern_length + pad) / 4;

    buf.put_u8(Opcodes::ListFonts as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(request_length); // request length
    buf.put_u16_le(1000); // max-names
    buf.put_u16_le(pattern_length); // length of pattern
    buf.put_slice(&[b'*']); // pattern

    buf.put_bytes(0, pad as usize);
}

fn query_extension(buf: &mut impl BufMut, extension_name: &[u8]) {
    buf.put_u8(Opcodes::QueryExtension as u8); // opcode
    buf.put_u8(0); // padding
    let n = extension_name.len();
    let p = pad(n);
    buf.put_u16_le((2 + (n + p) / 4).try_into().unwrap()); // request length
    buf.put_u16_le(n.try_into().unwrap()); // length of name
    buf.put_u16_le(0); // unused
    buf.put_slice(extension_name);
    buf.put_bytes(0, p);
}

#[derive(Debug)]
struct QueryExtensionReply {
    sequence_number: u16,
    reply_length: u32,
    present: bool,
    major_opcode: u8,
    first_event: u8,
    first_error: u8,
}

fn list_extensions(buf: &mut impl BufMut) {
    buf.put_u8(Opcodes::ListExtensions as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(1); // request length
}

fn open_font(buf: &mut impl BufMut, id_generator: &mut impl Iterator<Item = u32>) -> u32 {
    let font_name_length = 5;
    let font_id = id_generator.next().unwrap();
    buf.put_u8(Opcodes::OpenFont as u8); // opcode
    buf.put_u8(0); // padding
    buf.put_u16_le(3 + (font_name_length + pad(font_name_length as usize)) as u16 / 4); // request length
    buf.put_u32_le(font_id); // font ID
    buf.put_u16_le(font_name_length as u16); // length of name
    buf.put_u16_le(0); // unused
    buf.put_slice(b"fixed"); // name of font
    buf.put_bytes(0, pad(font_name_length as usize));

    font_id
}

fn image_text_8(buf: &mut impl BufMut, window_id: u32, gc_id: u32, x: u16, y: u16) {
    let text_name_length = 11;
    buf.put_u8(Opcodes::ImageText8 as u8); // opcode
    buf.put_u8(text_name_length as u8); // length of string
    buf.put_u16_le(4 + (text_name_length + pad(text_name_length as usize)) as u16 / 4); // request length
    buf.put_u32_le(window_id); // drawable
    buf.put_u32_le(gc_id); // context
    buf.put_u16_le(x); // x
    buf.put_u16_le(y); // y
    buf.put_slice(b"Hello World");
    unsafe { buf.advance_mut(pad(text_name_length as usize)) };
}

fn decode_event(event: Events, buf: &mut impl Buf) {
    eprintln!("event: {event:?}");
    if buf.remaining() < 31 {
        return;
    }

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

            eprintln!("keycode: {detail}");
        }
        Events::ButtonPress | Events::ButtonRelease => {
            let detail = buf.get_u8(); // keycode
            let sequence_number = buf.get_u16_le();
            let timestamp = buf.get_u32_le();

            buf.advance(24);

            eprintln!("button: {detail}");
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
                "sequence_number: {sequence_number}, request: {request}, key_code: {key_code}, count: {count}",
            );
            buf.advance(25); // unused
        }
        Events::Expose => {
            buf.advance(1); // unused
            let sequence_number = buf.get_u16_le();
            let window = buf.get_u32_le();
            let x = buf.get_u16_le();
            let y = buf.get_u16_le();
            let width = buf.get_u16_le();
            let height = buf.get_u16_le();
            buf.advance(16); // decode later
            eprintln!("window: {window}, x: {x}, y: {y}, width: {width}, height: {height}");
        }
        _ => panic!("unable to decode event yet: {event:?}"),
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
        Self {
            last: 0,
            max: mask,
            base,
            inc: mask & (!mask + 1),
        }
    }
}

impl Iterator for IdGenerator {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        // naive implementation for now

        if self.last == self.max {
            return None;
        }

        self.last += self.inc;

        Some(self.last | self.base)
    }
}

#[repr(u8)]
enum ShapeKind {
    Bounding = 0,
    Clip = 1,
    Input = 2,
}

#[repr(u8)]
enum ShapeOperations {
    Set = 0,
    Union = 1,
    Intersect = 2,
    Subtract = 3,
    Invert = 4,
}

struct ShapeExtension {
    major_opcode: u8,
}

impl ShapeExtension {
    fn new(major_opcode: u8) -> Self {
        Self { major_opcode }
    }

    fn query_version(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.major_opcode); // opcode
        buf.put_u8(0); // shape opcode
        buf.put_u16_le(1); // request length
    }

    fn rectangles(&self, buf: &mut impl BufMut, window_id: WindowId, x_offset: u16, y_offset: u16) {
        buf.put_u8(self.major_opcode); // opcode
        buf.put_u8(1); // shape opcode
        buf.put_u16_le(0); // request length
        buf.put_u8(ShapeOperations::Set as u8); // shape operation
        buf.put_u8(ShapeKind::Clip as u8); // destination kind
        buf.put_u8(0); // ordering
        unsafe { buf.advance_mut(1) };
        buf.put_u32_le(window_id);

        buf.put_u16_le(x_offset);
        buf.put_u16_le(y_offset);
    }

    fn mask(
        &self,
        buf: &mut impl BufMut,
        window_id: WindowId,
        x_offset: u16,
        y_offset: u16,
        pixmap_id: Option<PixmapId>,
    ) {
        buf.put_u8(self.major_opcode); // opcode
        buf.put_u8(2); // shape opcode
        buf.put_u16_le(5); // request length
        buf.put_u8(ShapeOperations::Set as u8); // shape operation
        buf.put_u8(ShapeKind::Clip as u8); // destination kind
        unsafe { buf.advance_mut(2) };
        buf.put_u32_le(window_id);

        buf.put_u16_le(x_offset);
        buf.put_u16_le(y_offset);

        if let Some(pixmap_id) = pixmap_id {
            buf.put_u32_le(pixmap_id); // source bitmap
        } else {
            buf.put_u32_le(0); // source bitmap
        }
    }

    fn combine(&self) {}

    fn offset(&self) {}

    fn query_extends(&self) {}

    fn select_input(&self) {}

    fn input_selected(&self) {}

    fn get_rectangles(&self) {}
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let matches = Command::new(crate_name!())
        .version(crate_version!())
        .arg(
            Arg::new("display")
                .help("display to use")
                .long("display")
                .value_name("DISPLAY")
                .value_parser(value_parser!(u32)),
        )
        .get_matches();

    let display = matches
        .get_one::<u32>("display")
        .map_or("1".to_string(), ToString::to_string);

    let mut stream = UnixStream::connect(String::from("/tmp/.X11-unix/X") + &display).await?; // Xnest server
    let mut connection_req = BytesMut::with_capacity(12);
    connection_req.put_u8(0x6c); // little endian byte order (LSB first)
    connection_req.put_u8(0); // unused
    connection_req.put_u16_le(11); // protocol major version
    connection_req.put_u16_le(0); // protocol minor version
    connection_req.put_u16_le(0); // length of authorization-protocol-name
    connection_req.put_u16_le(0); // length of authorization-protocol-data
    connection_req.put_u16_le(0);
    stream.write_all_buf(&mut connection_req).await?;

    let mut response = BytesMut::new();
    let n = stream.read_buf(&mut response).await?;
    let status_code = response.get_u8();
    match status_code {
        0 => panic!("failed"),
        1 => eprintln!("{}", "success".green()),
        2 => eprintln!("authenticate"),
        x => panic!("unknown response status code: {x}"),
    }

    response.advance(1); // unused pad

    let protocol_major_version = response.get_u16_le();
    let protocol_minor_version = response.get_u16_le();

    eprintln!("version major: {protocol_major_version}, minor: {protocol_minor_version}");

    let additional_data_len = response.get_u16_le();
    eprintln!("additional data len: {} [bytes]", additional_data_len * 4);

    while response.remaining() < additional_data_len as usize * 4 {
        stream.read_buf(&mut response).await?;
    }

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

    eprintln!("number of screens: {number_screens_roots}, number of formats: {number_formats}");

    let image_byte_order = match response.get_u8() {
        0 => ImageByteOrder::LSBFirst,
        1 => ImageByteOrder::MSBFirst,
        x => panic!("unknown image byte order {x}"),
    };

    let bitmap_format_bit_order = match response.get_u8() {
        0 => BitmapFormatBitOrder::LeastSignificant,
        1 => BitmapFormatBitOrder::MostSignificant,
        x => panic!("unknown bitmap format bit order {x}"),
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
                other => panic!("unknown backing store code {other}"),
            },
            save_unders: match response.get_u8() {
                0 => false,
                1 => true,
                other => panic!("save unders must be either 0 or 1, but is {other}"),
            },
            root_depth: response.get_u8(),
            number_depths_in_allowed_depths: response.get_u8(),
            allowed_depths: Vec::new(),
        });
        let last_screen = &mut *screen_roots.last_mut().unwrap();
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

            let last_allowed_depth = &mut *last_screen.allowed_depths.last_mut().unwrap();
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
                        other => panic!("unknown visual class {other}"),
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
    eprintln!("{screen:?}");
    eprintln!(
        "remaining from response: {} {} {}",
        response.remaining(),
        additional_data_len * 4,
        n
    );

    let (mut read_stream, write_stream) = stream.into_split();
    let (tx, mut rx): (
        tokio::sync::mpsc::Sender<(Opcodes, oneshot::Sender<Bytes>)>,
        tokio::sync::mpsc::Receiver<(Opcodes, oneshot::Sender<Bytes>)>,
    ) = mpsc::channel(1);

    let mut stream = write_stream;
    tokio::spawn(async move {
        let mut response_buf = BytesMut::new();
        loop {
            // Every reply contains a 32-bit length field expressed in units
            // of four bytes. Every reply consists of 32 bytes followed by
            // zero or more additional bytes of data, as specified in the
            // length field. Unused bytes within a reply are not guaranteed to
            // be zero. Every reply also contains the least significant 16
            // bits of the sequence number of the corresponding request.
            let n = read_stream.read_buf(&mut response_buf).await;
            while response_buf.remaining() >= 32 {
                let first_byte = response_buf.get_u8();

                if first_byte == 0 {
                    // Error
                    let raw_error_code = response_buf.get_u8();
                    eprintln!("raw_error_code: {raw_error_code}");
                    let error_code = ErrorCode::from_u8(raw_error_code).expect("valid error code");
                    eprintln!("code field: {error_code:?}");
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
                    // process replies
                    let reply_info = rx.recv().await;
                    if let Some((opcode, one_tx)) = reply_info {
                        eprintln!("received reply: {response_buf:?}, opcode: {opcode:?}");
                        match opcode {
                            Opcodes::GetWindowAttributes => {
                                while response_buf.remaining() < 44 {
                                    let _ = read_stream.read_buf(&mut response_buf).await;
                                }
                                let _ = one_tx.send(response_buf.split_to(43).freeze());
                            }
                            Opcodes::ListExtensions => {
                                let number_of_strings = response_buf.get_u8();
                                let sequence_number = response_buf.get_u16_le();
                                let response_length = response_buf.get_u32_le() as usize;
                                // unused, we can safely do that,
                                // because replies are at least 32
                                // bytes long
                                response_buf.advance(24);
                                while response_buf.remaining() < (response_length * 4) {
                                    let _ = read_stream.read_buf(&mut response_buf).await;
                                }
                                dbg!(&response_buf);

                                let mut sum_bytes = 0;
                                for string_nr in 0..number_of_strings {
                                    let str_len = response_buf.get_u8() as usize;
                                    let ascii_str = AsciiString::from_ascii(
                                        response_buf.get(..str_len).unwrap(),
                                    )
                                    .unwrap();
                                    response_buf.advance(str_len);
                                    println!("{ascii_str}");
                                    sum_bytes += 1 + str_len;
                                }
                                let _ = one_tx.send(response_buf.split_to(pad(sum_bytes)).freeze());
                            }
                            Opcodes::QueryExtension => {
                                let _ = one_tx.send(response_buf.split_to(31).freeze());
                            }
                            Opcodes::ListFonts => {
                                response_buf.advance(1); // ignore unused bytes
                                let _ = response_buf.get_u16_le(); // sequence number
                                let response_length = response_buf.get_u32_le() as usize;
                                while response_buf.remaining() < (response_length * 4 + 24) {
                                    let _ = read_stream
                                        .read_buf(&mut response_buf)
                                        .await
                                        .map_err(|_| 32u32)?;
                                }
                                let _ = one_tx
                                    .send(response_buf.split_to(response_length * 4 + 24).freeze());
                            }
                            Opcodes::OpenFont | Opcodes::ImageText8 => {
                                eprintln!("HERE");
                            }
                            _ => panic!("unknown opcode {opcode:?}"),
                        }
                    }
                } else if let Some(event) = Events::from_u8(first_byte) {
                    // process events
                    decode_event(event, &mut response_buf);
                } else {
                    panic!("unknown first byte {first_byte}");
                }
            }
        }

        Ok::<(), u32>(())
    });

    let mut id_generator = IdGenerator::new(resource_id_base, resource_id_mask);

    let mut request_buf = BytesMut::new();

    let window_id = create_window_request(&mut request_buf, &connection, screen, &mut id_generator);
    stream.write_all_buf(&mut request_buf).await?;

    map_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    get_window_attributes_request(&mut request_buf, window_id);
    let (one_tx, one_rx) = oneshot::channel();
    tx.send((Opcodes::GetWindowAttributes, one_tx)).await?;
    stream.write_all_buf(&mut request_buf).await?;
    let reply = WindowAttributesReply::from_bytes(&mut one_rx.await?);
    eprintln!("window attributes reply: {reply:?}");

    list_fonts(&mut request_buf);
    let (one_tx, one_rx) = oneshot::channel();
    tx.send((Opcodes::ListFonts, one_tx)).await?;
    stream.write_all_buf(&mut request_buf).await?;
    let mut list_fonts_bytes: Bytes = one_rx.await?;

    let mut number_of_names = list_fonts_bytes.get_u16_le();
    list_fonts_bytes.advance(22); // unused bytes

    while number_of_names > 0 {
        let font_string_length = list_fonts_bytes.get_u8();
        println!(
            "{}",
            AsciiString::from_ascii(
                list_fonts_bytes
                    .get(..(font_string_length as usize))
                    .unwrap(),
            )
            .unwrap()
        );

        list_fonts_bytes.advance(font_string_length as usize);

        number_of_names -= 1;
    }

    let font_id = open_font(&mut request_buf, &mut id_generator);
    stream.write_all_buf(&mut request_buf).await?;

    let gc_id = create_gc(
        &mut request_buf,
        &connection,
        screen.window,
        font_id,
        &mut id_generator,
    );
    stream.write_all_buf(&mut request_buf).await?;

    image_text_8(&mut request_buf, window_id, gc_id, 50, 50);
    stream.write_all_buf(&mut request_buf).await?;

    list_extensions(&mut request_buf);
    let (one_tx, one_rx) = oneshot::channel();
    tx.send((Opcodes::ListExtensions, one_tx)).await?;
    stream.write_all_buf(&mut request_buf).await?;
    one_rx.await?;

    query_extension(&mut request_buf, &b"SHAPE"[..]);
    let (one_tx, one_rx) = oneshot::channel();
    tx.send((Opcodes::QueryExtension, one_tx)).await?;
    stream.write_all_buf(&mut request_buf).await?;
    let mut query_extension_bytes: Bytes = one_rx.await?;
    query_extension_bytes.advance(1);
    let reply = QueryExtensionReply {
        sequence_number: query_extension_bytes.get_u16_le(),
        reply_length: query_extension_bytes.get_u32_le(),
        present: query_extension_bytes.get_u8() != 0,
        major_opcode: query_extension_bytes.get_u8(),
        first_event: query_extension_bytes.get_u8(),
        first_error: query_extension_bytes.get_u8(),
    };

    eprintln!(
        "present: {}, major_opcode: {}, base_event: {}",
        reply.present, reply.major_opcode, reply.first_event
    );

    query_extension(&mut request_buf, &b"Generic Event Extension"[..]);
    let (one_tx, one_rx) = oneshot::channel();
    tx.send((Opcodes::QueryExtension, one_tx)).await?;
    stream.write_all_buf(&mut request_buf).await?;
    let mut query_extension_bytes: Bytes = one_rx.await?;
    query_extension_bytes.advance(1);
    let reply = QueryExtensionReply {
        sequence_number: query_extension_bytes.get_u16_le(),
        reply_length: query_extension_bytes.get_u32_le(),
        present: query_extension_bytes.get_u8() != 0,
        major_opcode: query_extension_bytes.get_u8(),
        first_event: query_extension_bytes.get_u8(),
        first_error: query_extension_bytes.get_u8(),
    };
    eprintln!("generic event extension: {reply:?}");

    for i in 0..100 {
        eprintln!("{i}");
        sleep(Duration::from_millis(200)).await;
        configure_window(
            &mut request_buf,
            window_id,
            &[ConfigureWindowCommands::X(5), ConfigureWindowCommands::Y(5)],
            2 * i,
            0,
        );
        stream.write_all_buf(&mut request_buf).await?;
    }

    free_gc(&mut request_buf, gc_id);
    stream.write_all_buf(&mut request_buf).await?;

    close_font(&mut request_buf, font_id);
    stream.write_all_buf(&mut request_buf).await?;

    unmap_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    destroy_window_request(&mut request_buf, window_id);
    stream.write_all_buf(&mut request_buf).await?;

    Ok(())
}
