// Imports
use anyhow::Context;
use futures::AsyncWriteExt;
use gtk4::{gdk, gio, prelude::*};
use palette::convert::IntoColor;
use path_absolutize::Absolutize;
use rnote_compose::Color;
use std::cell::Ref;
use std::path::Path;
use std::slice::Iter;

/// The suffix delimiter when duplicating/renaming already existing files
pub(crate) const FILE_DUP_SUFFIX_DELIM: &str = " - ";
/// The suffix delimiter when duplicating/renaming already existing files for usage in a regular expression
pub(crate) const FILE_DUP_SUFFIX_DELIM_REGEX: &str = r"\s-\s";

/// Create a new file or replace if it already exists, asynchronously.
pub(crate) async fn create_replace_file_future(
    bytes: Vec<u8>,
    file: &gio::File,
) -> anyhow::Result<()> {
    let Some(file_path) = file.path() else {
        return Err(anyhow::anyhow!(
            "Can't create-replace file that has no path."
        ));
    };
    let mut write_file = async_fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&file_path)
        .await
        .context(format!(
            "Failed to create/open/truncate file for path '{}'",
            file_path.display()
        ))?;
    write_file.write_all(&bytes).await.context(format!(
        "Failed to write bytes to file with path '{}'",
        file_path.display()
    ))?;
    write_file.sync_all().await.context(format!(
        "Failed to sync file after writing with path '{}'",
        file_path.display()
    ))?;
    Ok(())
}

pub(crate) fn str_from_u8_nul_utf8(utf8_src: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let nul_range_end = utf8_src
        .iter()
        .position(|&c| c == b'\0')
        .unwrap_or(utf8_src.len()); // default to length if no `\0` present
    std::str::from_utf8(&utf8_src[0..nul_range_end])
}

/// Get the index of the AxisUse enum
///
/// TODO: Report to gtk-rs that [gdk::AxisUse] needs a [`Into<std::ops::Index>`] implementation
/// for usage to retrieve pointer axes in [gdk::TimeCoord]
pub(crate) fn axis_use_idx(a: gdk::AxisUse) -> usize {
    match a {
        gdk::AxisUse::Ignore => 0,
        gdk::AxisUse::X => 1,
        gdk::AxisUse::Y => 2,
        gdk::AxisUse::DeltaX => 3,
        gdk::AxisUse::DeltaY => 4,
        gdk::AxisUse::Pressure => 5,
        gdk::AxisUse::Xtilt => 6,
        gdk::AxisUse::Ytilt => 7,
        gdk::AxisUse::Wheel => 8,
        gdk::AxisUse::Distance => 9,
        gdk::AxisUse::Rotation => 10,
        gdk::AxisUse::Slider => 11,
        _ => unreachable!(),
    }
}

pub(crate) fn default_file_title_for_export(
    output_file: Option<gio::File>,
    fallback: Option<&str>,
    suffix: Option<&str>,
) -> String {
    let mut title = output_file
        .and_then(|f| Some(f.basename()?.file_stem()?.to_string_lossy().to_string()))
        .unwrap_or_else(|| {
            fallback
                .map(|f| f.to_owned())
                .unwrap_or_else(rnote_engine::utils::now_formatted_string)
        });

    if let Some(suffix) = suffix {
        title += suffix;
    }

    title
}

/// Absolutizes two paths and checks if they are equal.
///
/// Compared to `canonicalize()`, the files do not need to exist on the fs and symlinks are not resolved.
#[inline]
pub(crate) fn paths_abs_eq(
    first: impl AsRef<Path>,
    second: impl AsRef<Path>,
) -> anyhow::Result<bool> {
    let first = first.as_ref().absolutize()?.into_owned();
    let second = second.as_ref().absolutize()?.into_owned();
    Ok(first == second)
}

/// Wrapper type that enables iterating over [`std::cell::RefCell<Vec<T>>`]
pub(crate) struct VecRefWrapper<'a, T: 'a> {
    r: Ref<'a, Vec<T>>,
}

impl<'a, 'b: 'a, T: 'a> IntoIterator for &'b VecRefWrapper<'a, T> {
    type IntoIter = Iter<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Iter<'a, T> {
        self.r.iter()
    }
}

impl<'a, T> VecRefWrapper<'a, T>
where
    T: 'a,
{
    pub(crate) fn new(r: Ref<'a, Vec<T>>) -> Self {
        Self { r }
    }
}

/// Create a string for display the hue, saturation and value properties of the color.
pub(crate) fn color_to_hsv_label_string(color: Color) -> String {
    let palette_color: palette::Okhsv<f64> = color.into_color();
    let alpha = color.a;
    let hue = palette_color.hue.into_inner();
    let saturation = palette_color.saturation;
    let value = palette_color.value;

    // Since the color might come from gdk which only has f32 precision, let's use f32's epsilon
    let min_saturated =
        approx::relative_eq!(saturation, 0.0, epsilon = f32::EPSILON as f64) || saturation <= 0.0;
    //let max_saturated = approx::relative_eq!(saturation, 1.0, epsilon = f32::EPSILON as f64) || saturation >= 1.0;
    let min_bright =
        approx::relative_eq!(value, 0.0, epsilon = f32::EPSILON as f64) || value <= 0.0;
    let max_bright =
        approx::relative_eq!(value, 1.0, epsilon = f32::EPSILON as f64) || value >= 1.0;
    let min_alpha = approx::relative_eq!(alpha, 0.0, epsilon = f32::EPSILON as f64) || alpha <= 0.0;
    //let max_alpha = approx::relative_eq!(alpha, 1.0, epsilon = f32::EPSILON as f64) || alpha >= 1.0;

    // The ranges are tweaked by hand, because the perceived color might not always match slices of 30deg of the value
    // pie exactly.
    let hue_str = match hue {
        _ if min_saturated => "grey",
        v if v < 0.0 => "rose",
        v if (0.0..40.0).contains(&v) => "red",
        v if (40.0..80.0).contains(&v) => "orange",
        v if (80.0..108.0).contains(&v) => "yellow",
        v if (108.0..120.0).contains(&v) => "chartreuse-green",
        v if (120.0..150.0).contains(&v) => "green",
        v if (150.0..180.0).contains(&v) => "spring-green",
        v if (180.0..210.0).contains(&v) => "cyan",
        v if (210.0..240.0).contains(&v) => "azure",
        v if (240.0..280.0).contains(&v) => "blue",
        v if (280.0..315.0).contains(&v) => "violet",
        v if (315.0..345.0).contains(&v) => "magenta",
        v if v >= 345.0 => "rose",
        _ => unreachable!(),
    };
    let saturation_str = match saturation {
        _ if min_saturated => "".to_string(),
        v if v < 0.25 => String::from("greyish"),
        v if (0.25..0.50).contains(&v) => "".to_string(),
        v if (0.50..0.75).contains(&v) => "strong".to_string(),
        v if v >= 0.75 => String::from("vivid"),
        _ => unreachable!(),
    };
    let value_str = match value {
        v if v < 0.25 =>  String::from("very-dark"),
        v if (0.25..0.50).contains(&v) => String::from("dark"),
        v if (0.50..0.75).contains(&v) => String::from("mid"),
        v if v >= 0.667 => String::from("bright"),
        _ => unreachable!(),
    };
    let alpha_str = match alpha {
        v if v < 0.333 => "transparent",
        v if (0.333..0.667).contains(&v) => "translucent",
        v if (0.667..1.0).contains(&v) => "slightly-translucent",
        v if v >= 1.0 => "",
        _ => unreachable!(),
    };

    if min_alpha {
        "fully transparent".to_string()
    } else if min_saturated && min_bright {
        "black".to_string()
    } else if min_saturated && max_bright {
        "white".to_string()
    } else {
        format!("{alpha_str} {saturation_str} {value_str} {hue_str}")
    }
}
