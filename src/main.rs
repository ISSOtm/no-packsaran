use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use palette::{FromColor, Hsl, Srgb};
use plumers::prelude::*;

fn main() -> ExitCode {
    let cli = xflags::parse_or_exit! {
        /// How large your colour palettes are. Defaults to 4 (2bpp).
        optional -s,--palette-size nb_colors: usize
        /// How large your tiles are (they are assumed to be square). Defaults to 8.
        optional -T,--tile-size pixels: usize
        /// Strategy to defeat. Either `any_fit` or `best_fusion`.
        required strategy: String
        /// Where to write the Evil™ image to.
        required out_path: PathBuf
    };
    let palette_size = cli.palette_size.unwrap_or(4);
    let tile_size = cli.tile_size.unwrap_or(8);

    let (tile_colors, nb_colors) = if cli.strategy == "any_fit" {
        let nb_colors = palette_size & !2; // Round down to the nearest even number.
        defeat_any_fit(nb_colors)
    } else if cli.strategy == "best_fusion" {
        todo!()
    } else {
        eprintln!("Error: Unknown strategy \"{}\"", cli.strategy);
        return ExitCode::FAILURE;
    };

    gen_image(tile_size, nb_colors, tile_colors, &cli.out_path);

    ExitCode::SUCCESS
}

/// Strategy:
///
/// 1. Divide the colours into two palette-sized disjoint sets. (We will use even and odd indices.)
/// 2. Make sure that each tile only has colours from either set (never both!),
///    and that these “proto-palettes” are fed to the packing algorithm alternatingly.
///
/// The image can be displayed using just the two sets, but the greediness of these algorithms
/// makes them generate N palettes composed of one “proto-palette” from each set.
fn defeat_any_fit(palette_size: usize) -> (Vec<Vec<u8>>, u8) {
    let nb_colors_per_tile = palette_size / 2;
    let nb_colors =
        u8::try_from(palette_size * 2).expect("Color indices are only supported up to 256!");

    (
        combination::combine::index(palette_size, nb_colors_per_tile)
            .into_iter()
            .flat_map(|combination| {
                let (evens, odds) = combination
                    .into_iter()
                    .map(|index| {
                        let even = (index * 2) as u8;
                        (even, even + 1)
                    })
                    .collect();
                [evens, odds]
            })
            .collect(),
        nb_colors,
    )
}

/// Generates tile data
fn gen_image(tile_size: usize, nb_colors: u8, tile_colors: Vec<Vec<u8>>, path: &Path) {
    let palette = (0..nb_colors).map(|i| {
        let (red, green, blue) = Srgb::from_color(Hsl::new_srgb(
            f32::from(i) / f32::from(nb_colors) * 360.,
            1.0, // Max saturation.
            0.5, // Clear colour.
        ))
        .into_format()
        .into_components();
        Rgb32(u32::from_le_bytes([red, green, blue, 0xFF]))
    });

    let mut img = PalettedImage32::new_zeroed(
        ImageFormat::Png,
        AlphaMode::ZeroIsTransparent,
        1,
        tile_size,
        tile_size * tile_colors.len(),
        palette,
    )
    .unwrap();
    let mut frame = img.frame_mut(0);
    for (i, tile) in tile_colors.iter().enumerate() {
        for y in 0..tile_size {
            let dest_y = i * tile_size + y;
            for x in 0..tile_size {
                frame[(x, dest_y)] = tile[(x + y * tile_size) % tile.len()];
            }
        }
    }

    match img.store(path) {
        Ok(_nb_bytes_written) => {} // OK
        Err(err) => {
            eprintln!("Failed to write image to \"{}\": {err}", path.display());
            std::process::exit(1);
        }
    }
}
