use std::{
    error::Error,
    fmt::Debug,
    fs::File,
    io::{BufRead, BufWriter, Read},
};

use deku::{ctx::Limit, DekuContainerRead, DekuRead};
use derivative::Derivative;

use image::{codecs::gif::GifEncoder, Delay, Frame, ImageOutputFormat, RgbaImage};
use object::{write::Relocation, ObjectSection};

pub struct BoundedString {
    data: Vec<u8>,
}

/// Passthrough deku read to Vec<u8> default Limit reader
impl<'a, T: DekuRead<'a, Ctx>, Ctx: Copy, Predicate: FnMut(&T) -> bool>
    DekuRead<'a, (Limit<T, Predicate>, Ctx)> for BoundedString
where
    Vec<u8>: DekuRead<'a, (Limit<T, Predicate>, Ctx)>,
{
    fn read(
        input: &'a deku::bitvec::BitSlice<u8, deku::bitvec::Msb0>,
        ctx: (Limit<T, Predicate>, Ctx),
    ) -> Result<(&'a deku::bitvec::BitSlice<u8, deku::bitvec::Msb0>, Self), deku::DekuError>
    where
        Self: Sized,
    {
        Vec::<u8>::read(input, ctx).map(|x| (x.0, Self { data: x.1 }))
    }
}

impl BoundedString {
    pub fn str(&self) -> String {
        String::from_utf8_lossy(
            self.data
                .iter()
                .cloned()
                .filter(|p| *p != 0)
                .collect::<Vec<u8>>()
                .as_slice(),
        )
        .to_string()
    }
}

impl Debug for BoundedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.str().fmt(f)
    }
}

// UTF 16 string
#[derive(DekuRead)]
#[deku(endian = "little")]
pub struct BoundedUTF16String {
    data: [u16; 128],
}

impl BoundedUTF16String {
    pub fn str(&self) -> String {
        String::from_utf16_lossy(
            self.data
                .iter()
                .cloned()
                .filter(|p| *p != 0)
                .collect::<Vec<u16>>()
                .as_slice(),
        )
        .to_string()
    }
}

impl Debug for BoundedUTF16String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.str().fmt(f)
    }
}
// https://dsibrew.org/wiki/DSi_cartridge_header

#[derive(Derivative, DekuRead)]
#[derivative(Debug)]
#[deku(endian = "little")]
pub struct NDSHeader {
    #[deku(count = "12")]
    pub title: BoundedString,
    #[deku(count = "4")]
    pub gamecode: BoundedString,
    pub makercode: u16,
    pub unitcode: u8,
    pub encryption_seed: u8,
    pub device_capacity: u8,
    #[deku(count = "7")]
    #[derivative(Debug = "ignore")]
    pub reserved_1: Vec<u8>,
    pub gamerevision: u16,
    pub romversion: u8,
    pub autostart: u8,

    pub arm9_offset: u32,
    pub arm9_entry: u32,
    pub arm9_load: u32,
    pub arm9_size: u32,

    pub arm7_offset: u32,
    pub arm7_entry: u32,
    pub arm7_load: u32,
    pub arm7_size: u32,

    pub fnt_offset: u32,
    pub fnt_size: u32,

    pub fat_offset: u32,
    pub fat_size: u32,

    pub arm9_overlay_offset: u32,
    pub arm9_overlay_size: u32,

    pub arm7_overlay_offset: u32,
    pub arm7_overlay_length: u32,

    pub _1: u32,
    pub _2: u32,
    pub icon_banner_offset: u32,
    pub secure_area_crc: u16,
    pub secure_tfr_timeout: u16,

    pub arm9_autoload: u32,
    pub arm7_autoload: u32,

    pub secure_disable: u64,
    pub ntr_region_size: u32,
    pub header_size: u32,

    #[deku(count = "56")]
    #[derivative(Debug = "ignore")]
    pub reserved_2: Vec<u8>,
    #[deku(count = "156")]
    #[derivative(Debug = "ignore")]
    pub logo: Vec<u8>,

    pub logo_crc: u16,
    pub header_crc: u16,

    #[deku(count = "32")]
    #[derivative(Debug = "ignore")]
    pub debugger_reserved: Vec<u8>,
}

// no$gba documentation Nocash @ http://www.problemkaputt.de/gba.htm

#[derive(Derivative, DekuRead)]
#[derivative(Debug)]
#[deku(endian = "little")]
pub struct DSIIconFrame {
    #[derivative(Debug = "ignore")]
    pub data: [u8; 0x200],
}

#[derive(Derivative, DekuRead)]
#[derivative(Debug)]
pub struct NDSIconPalette {
    #[derivative(Debug = "ignore")]
    pub colors: [u16; 16],
}

#[derive(Derivative, DekuRead)]
#[derivative(Debug)]
pub struct DSIIcon {
    #[derivative(Debug = "ignore")]
    pub frames: [DSIIconFrame; 8],
    #[derivative(Debug = "ignore")]
    pub frame_palettes: [NDSIconPalette; 8],
    #[derivative(Debug = "ignore")]
    pub sequence: [u16; 64],
}

#[derive(Derivative, DekuRead)]
#[derivative(Debug)]
pub struct NDSIcon {
    pub version: u16,
    pub crc_1: u16,
    pub crc_2: u16,
    pub crc_3: u16,
    pub crc_4: u16,

    #[deku(count = "22")]
    #[derivative(Debug = "ignore")]
    pub reserved: Vec<u8>,

    #[derivative(Debug = "ignore")]
    pub bitmap: [u8; 0x200],

    #[derivative(Debug = "ignore")]
    pub palette: NDSIconPalette,

    pub title_jp: BoundedUTF16String,
    pub title_en: BoundedUTF16String,
    pub title_fr: BoundedUTF16String,
    pub title_de: BoundedUTF16String,
    pub title_it: BoundedUTF16String,
    pub title_es: BoundedUTF16String,
    pub title_zh: BoundedUTF16String,
    pub title_kr: BoundedUTF16String,
    #[deku(count = "2048")]
    #[derivative(Debug = "ignore")]
    pub zerofilled: Vec<u8>,

    #[deku(cond = "*version >= 259")]
    pub dsi_icon: Option<DSIIcon>,
}

/// very basic, just converts the contents of arm7 & arm9 to elfs, some more work is needed
fn dump_elf(header: &NDSHeader, content: &[u8]) -> Result<(), Box<dyn Error>> {
    let arm9: Vec<u8> = content
        .iter()
        .skip(header.arm9_offset as usize)
        .take(header.arm9_size as usize)
        .cloned()
        .collect();

    let arm7: Vec<u8> = content
        .iter()
        .skip(header.arm7_offset as usize)
        .take(header.arm7_size as usize)
        .cloned()
        .collect();

    let mut arm9_obj = object::write::Object::new(
        object::BinaryFormat::Elf,
        object::Architecture::Arm,
        object::Endianness::Little,
    );

    let arm9_section = arm9_obj.add_section(
        "arm9".to_string().into_bytes(),
        "arm9".to_string().into_bytes(),
        object::SectionKind::Text,
    );

    arm9_obj.set_section_data(arm9_section, arm9.as_slice(), 4);

    arm9_obj.write_stream(std::io::BufWriter::new(File::create("out9.elf")?))?;
    println!("arm9 elf written");

    let mut arm7_obj = object::write::Object::new(
        object::BinaryFormat::Elf,
        object::Architecture::Arm,
        object::Endianness::Little,
    );

    let arm7_section = arm7_obj.add_section(
        "arm7".to_string().into_bytes(),
        "arm7".to_string().into_bytes(),
        object::SectionKind::Text,
    );

    arm7_obj.set_section_data(arm7_section, arm7.as_slice(), 4);

    arm7_obj.write_stream(std::io::BufWriter::new(File::create("out7.elf")?))?;
    println!("arm7 elf written");

    Ok(())
}

/// 5bit color channel to 8 bit color channel
fn conv(single_channel: u16) -> u8 {
    (((single_channel as f64) / 31.) * 255.).round() as u8
}

#[derive(Debug, Default)]
pub struct IconAnimation {
    pub flip_vert: bool,
    pub flip_horiz: bool,
    pub palette: u8,
    pub bitmap: u8,
    pub duration: u8,
}

fn dump_icon_frame<Blit: FnMut(u32, u32, [u8; 4])>(
    bitmap: &[u8; 512],
    palette: &NDSIconPalette,
    mut blit: Blit,
    anim: IconAnimation,
) {
    let mut img = bitmap
        .into_iter()
        .map(|x| [x & 0b1111, x >> 4].into_iter())
        .flatten();

    for y in 0..4 {
        for x in 0..4 {
            let tile = img.clone().take(8 * 8).collect::<Vec<u8>>();
            let off = (x * 8, y * 8);

            // do all the tiles
            for x in 0..8 {
                for y in 0..8 {
                    let me = tile[(y * 8) + x];
                    let (px, py) = (x as u32 + off.0, y as u32 + off.1);
                    blit(
                        if anim.flip_horiz { 31 - px } else { px },
                        if anim.flip_vert { 31 - py } else { py },
                        if me == 0 {
                            [0, 0, 0, 0]
                        } else {
                            let color = palette.colors[me as usize];
                            [
                                conv((color) & 0b11111),
                                conv((color >> 5) & 0b11111),
                                conv((color >> 10) & 0b11111),
                                255,
                            ]
                        },
                    );
                }
            }

            img.advance_by(8 * 8).unwrap();
        }
    }
}

fn dump_icon(header: &NDSHeader, content: &[u8]) -> Result<(), Box<dyn Error>> {
    let icon = NDSIcon::from_bytes((content.split_at(header.icon_banner_offset as usize).1, 0))?.1;
    println!("{:#?}", icon);

    let mut icon_png = image::RgbaImage::new(32, 32);

    dump_icon_frame(
        &icon.bitmap,
        &icon.palette,
        |x, y, color| icon_png.get_pixel_mut(x, y).0 = color,
        IconAnimation::default(),
    );

    icon_png.write_to(
        &mut BufWriter::new(File::create(format!("icon_{}.png", header.gamecode.str()))?),
        ImageOutputFormat::Png,
    )?;

    if let Some(dsi) = icon.dsi_icon {
        let mut gif = GifEncoder::new(BufWriter::new(File::create(format!(
            "dsi_icon_{}.gif",
            header.gamecode.str()
        ))?));

        gif.set_repeat(image::codecs::gif::Repeat::Infinite)?;

        let mut frames = vec![];
        for seq in dsi.sequence {
            if seq == 0 {
                break;
            }

            let mut buffer = RgbaImage::new(32, 32);

            let anim = IconAnimation {
                flip_vert: ((seq >> 15) & 1) > 0,
                flip_horiz: ((seq >> 14) & 1) > 0,
                palette: ((seq >> 11) & 7) as u8,
                bitmap: ((seq >> 8) & 7) as u8,
                duration: (seq & 0xff) as u8,
            };

            let bitmap = &dsi.frames[anim.bitmap as usize];
            let palette = &dsi.frame_palettes[anim.palette as usize];

            let dur = anim.duration;
            dump_icon_frame(
                &bitmap.data,
                palette,
                |x, y, color| buffer.get_pixel_mut(x, y).0 = color,
                anim,
            );

            frames.push(Frame::from_parts(
                buffer,
                0,
                0,
                Delay::from_saturating_duration(std::time::Duration::from_millis(
                    (((dur as f64) / 60.) * 1000.) as u64,
                )),
            ));
        }

        gif.encode_frames(frames)?;
    }

    Ok(())
}

pub fn extract<T: BufRead>(mut nds_file: T) -> Result<NDSHeader, Box<dyn Error>> {
    let mut content: Vec<u8> = Vec::new();

    // TODO: make this streamed
    nds_file.read_to_end(&mut content)?;

    let header = NDSHeader::from_bytes((content.as_slice(), 0))?.1;
    println!("{:#?}", &header);
    dump_elf(&header, content.as_slice())?;
    dump_icon(&header, content.as_slice())?;

    Ok(header)
}
