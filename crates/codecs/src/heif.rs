//! HEIF / HEIC (the iPhone and Mac photo format), read-only.
//!
//! Decoding lives in the optional `craft-heif` crate (heic-rs, a pure-Rust HEVC
//! still-picture decoder, plus a libheif-matching colour conversion), enabled by this crate's
//! `heif` feature — off by default because HEVC is patent-encumbered and whether a build carries
//! an HEVC decoder is the distributor's call (the same policy as the sibling PhotoCraft). Without
//! the feature, HEIF files are still detected and opening one is an [`Error::Unsupported`] error
//! saying so, never a panic.
//!
//! HEIF records orientation in the container (`irot`/`imir`, plus a `clap` crop), not in EXIF,
//! so the decoder applies them and `orientation` is reported as 1: the EXIF Orientation tag only
//! mirrors what the container says, and applying it as well would turn the photo twice.
//! Some writers (photo editors and converters, not iPhones) leave the container unturned and give
//! the turn only in EXIF, as a JPEG would, and macOS shows those upright: when the file has no
//! `irot` or `imir` at all, its EXIF Orientation is reported instead.
//! The samples come back as 16-bit RGB in the container's own primaries, with its ICC profile
//! (iPhones: Display P3) or, without one, its `nclx` / VUI colour description; like every decoder
//! here, the colour interpretation happens in [`crate::convert::finish`].

#[cfg(feature = "heif")]
use crate::convert::{Buf, Meta, Model, Raw, finish};
use crate::{DecodeOptions, Decoded, Error, Format, Result};

const F: Format = Format::Heif;

/// The reason given when this build has no HEIF decoder.
#[cfg(not(feature = "heif"))]
pub(crate) const NOT_IN_BUILD: &str = "HEIC/HEIF support isn't included in this build of LightCraft";

#[cfg(not(feature = "heif"))]
pub(crate) fn decode(_bytes: &[u8], _opts: &DecodeOptions) -> Result<Decoded> {
    Err(Error::Unsupported(F, NOT_IN_BUILD))
}

#[cfg(not(feature = "heif"))]
pub(crate) fn header(_bytes: &[u8]) -> Result<(u32, u32, u16)> {
    Err(Error::Unsupported(F, NOT_IN_BUILD))
}

#[cfg(feature = "heif")]
fn err(e: craft_heif::Error) -> Error {
    match e {
        craft_heif::Error::Unsupported(why) => Error::Unsupported(F, why),
        craft_heif::Error::Limit(m) | craft_heif::Error::Malformed(m) => Error::Malformed(F, m),
    }
}

/// Most boxes looked at while searching for `irot` / `imir` (a real file has a few dozen).
#[cfg(feature = "heif")]
const MAX_BOXES: usize = 4096;

/// The boxes in `b`, a box payload: `(type, payload)` each, or `None` when one is malformed or
/// `budget` runs out.
#[cfg(feature = "heif")]
fn boxes<'a>(b: &'a [u8], budget: &mut usize) -> Option<Vec<([u8; 4], &'a [u8])>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < b.len() {
        *budget = budget.checked_sub(1)?;
        let head = b.get(at..at.checked_add(8)?)?;
        let kind = [head[4], head[5], head[6], head[7]];
        let (len, header) = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
            // to the end of the enclosing payload
            0 => (b.len() - at, 8),
            1 => {
                let l = b.get(at.checked_add(8)?..at.checked_add(16)?)?;
                (usize::try_from(u64::from_be_bytes([l[0], l[1], l[2], l[3], l[4], l[5], l[6], l[7]])).ok()?, 16)
            }
            n => (usize::try_from(n).ok()?, 8),
        };
        if len < header {
            return None;
        }
        let end = at.checked_add(len)?;
        out.push((kind, b.get(at + header..end)?));
        at = end;
    }
    Some(out)
}

/// Whether the container turns or mirrors an image: an `irot` or `imir` among the item properties
/// (`meta` ▸ `iprp` ▸ `ipco`). A file this walk can't follow counts as turned, which keeps the
/// container's word, as before.
// (craft-heif's `Info` doesn't say; it could, upstream)
#[cfg(feature = "heif")]
fn container_orients(bytes: &[u8]) -> bool {
    let mut budget = MAX_BOXES;
    let mut child = |payload: &[u8], kind: &[u8; 4]| -> Option<Vec<u8>> {
        boxes(payload, &mut budget)?.into_iter().find(|(k, _)| k == kind).map(|(_, p)| p.to_vec())
    };
    let Some(meta) = child(bytes, b"meta") else { return true };
    // `meta` is a full box: its version and flags come first
    let Some(iprp) = meta.get(4..).and_then(|m| child(m, b"iprp")) else { return true };
    let Some(ipco) = child(&iprp, b"ipco") else { return true };
    match boxes(&ipco, &mut budget) {
        Some(props) => props.iter().any(|(k, _)| k == b"irot" || k == b"imir"),
        None => true,
    }
}

/// The EXIF Orientation still to apply to the decoded picture: the tag's value when the container
/// never turns the picture itself, else 1 (the tag then only repeats the container).
#[cfg(feature = "heif")]
fn exif_orientation(bytes: &[u8], exif: Option<&[u8]>) -> u16 {
    if container_orients(bytes) {
        return 1;
    }
    exif.map(crate::exif::summarize).and_then(|s| s.orientation).filter(|o| (1..=8).contains(o)).unwrap_or(1)
}

/// The size as the container shows it (its transforms applied) and the EXIF Orientation still to
/// apply ([`exif_orientation`]), from the container alone: no picture is decoded.
#[cfg(feature = "heif")]
pub(crate) fn header(bytes: &[u8]) -> Result<(u32, u32, u16)> {
    let info = craft_heif::probe(bytes).map_err(err)?;
    let exif = craft_heif::read_metadata(bytes).ok().and_then(|(exif, _)| exif);
    Ok((info.width, info.height, exif_orientation(bytes, exif.as_deref())))
}

#[cfg(feature = "heif")]
pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded> {
    // The container alone: the declared size is checked before any pixel is decoded.
    let info = craft_heif::probe(bytes).map_err(err)?;
    if (info.width as u64).saturating_mul(info.height as u64) > opts.max_pixels {
        return Err(Error::TooLarge(info.width as u64, info.height as u64));
    }
    let options = craft_heif::Options { max_pixels: opts.max_pixels, apply_transforms: true, ..Default::default() };
    // A small decode (a thumbnail or a cataloguing probe) uses the file's own thumbnail image when
    // it is at least as large as asked: iPhones store one of 320 × 240, and decoding it is a
    // hundredth of the work of the 12 MP photo.
    let thumb = match opts.max_size {
        Some((mw, mh)) if mw > 0 && mh > 0 && mw.max(mh) < info.width.max(info.height) => {
            craft_heif::decode_thumbnail(bytes, mw.max(mh), &options).ok().flatten().filter(|t| {
                // Only a thumbnail of the same picture: same orientation and aspect within 1 %.
                let aspect = |w: u32, h: u32| w as f64 / h.max(1) as f64;
                (aspect(t.width, t.height) / aspect(info.width, info.height) - 1.0).abs() < 0.01
            })
        }
        _ => None,
    };
    let mut decoded = match thumb {
        Some(t) => t,
        None => craft_heif::decode(bytes, &options).map_err(err)?,
    };
    let raw = Raw {
        width: decoded.width as usize,
        height: decoded.height as usize,
        model: Model::Rgb,
        alpha: decoded.has_alpha,
        premultiplied: false,
        buf: Buf::U16(std::mem::take(&mut decoded.data)),
        bit_depth: decoded.bit_depth,
    };
    let c = decoded.colour;
    // (a thumbnail may come without the photo's EXIF: the file's own is read then)
    let orientation = match decoded.exif.as_deref() {
        Some(exif) => exif_orientation(bytes, Some(exif)),
        None => exif_orientation(bytes, craft_heif::read_metadata(bytes).ok().and_then(|(exif, _)| exif).as_deref()),
    };
    let meta = Meta {
        icc: decoded.icc,
        exif: decoded.exif,
        xmp: decoded.xmp,
        orientation: Some(orientation),
        // Used when there is no (usable) ICC profile.
        hint: crate::space::SourceSpace::from_cicp(c.primaries, c.transfer),
        ..Default::default()
    };
    finish(F, raw, meta, (info.width, info.height), opts)
}

#[cfg(test)]
mod tests {
    use crate::{Format, sniff};

    /// The container header both builds recognise, whatever the feature says.
    const HEIC_HEADER: &[u8] = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic";

    #[test]
    fn heif_is_always_recognised() {
        assert_eq!(sniff(HEIC_HEADER), Some(Format::Heif));
    }

    #[test]
    fn can_decode_follows_the_feature() {
        assert_eq!(Format::Heif.can_decode(), cfg!(feature = "heif"));
    }

    #[cfg(not(feature = "heif"))]
    #[test]
    fn without_the_feature_heif_is_a_clear_unsupported_error() {
        let r = crate::decode(HEIC_HEADER, crate::DecodeOptions::default());
        assert!(matches!(&r, Err(crate::Error::Unsupported(Format::Heif, why)) if why.contains("isn't included in this build")), "{r:?}");
        let h = crate::read_header(HEIC_HEADER);
        assert!(matches!(&h, Err(crate::Error::Unsupported(Format::Heif, why)) if why.contains("isn't included in this build")), "{h:?}");
    }

    /// Found by PhotoCraft's `decode_heif` fuzz target: a malformed box makes heic-rs 0.1.1 slice
    /// out of range (`boxes.rs:130`). It must be an error, not a crash.
    #[cfg(feature = "heif")]
    const HEIC_RS_BOX_PANIC: [u8; 72] = [
        0x00, 0x00, 0x00, 0x24, 0x66, 0x74, 0x79, 0x70, 0x68, 0x65, 0x69, 0x63, 0x00, 0x00, 0x00, 0x00, 0x6d, 0x69, 0x66, 0x31, 0x4d, 0x69, 0x50,
        0x72, 0x6d, 0x69, 0x61, 0x66, 0x4d, 0x69, 0x48, 0x42, 0x72, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6d, 0x65, 0x74, 0x61, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x75, 0x75, 0x69, 0x64, 0x00, 0x00, 0x00, 0x00, 0x03, 0x08, 0x08, 0x08, 0x00, 0x03, 0x01, 0x03, 0x70,
        0x00, 0xa8, 0x00,
    ];

    #[cfg(feature = "heif")]
    #[test]
    fn a_heic_rs_panic_is_a_malformed_error() {
        let r = crate::decode(&HEIC_RS_BOX_PANIC, crate::DecodeOptions::default());
        assert!(matches!(&r, Err(crate::Error::Malformed(Format::Heif, _))), "{r:?}");
        assert!(crate::read_header(&HEIC_RS_BOX_PANIC).is_err());
    }

    #[cfg(feature = "heif")]
    #[test]
    fn an_image_sequence_is_unsupported() {
        // An ftyp with no meta box: a video track, not a still.
        let r = crate::decode(b"\0\0\0\x18ftypmsf1\0\0\0\0msf1hevc", crate::DecodeOptions::default());
        assert!(matches!(&r, Err(crate::Error::Unsupported(Format::Heif, why)) if why.contains("sequence")), "{r:?}");
    }
}
