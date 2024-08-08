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
        let nb_colors = palette_size & !2; // Round down to the nearest even number.
        defeat_best_fusion(nb_colors)
    } else {
        eprintln!("Error: Unknown strategy \"{}\"", cli.strategy);
        return ExitCode::FAILURE;
    };

    gen_image(tile_size, nb_colors, tile_colors, &cli.out_path);

    ExitCode::SUCCESS
}

/// Strategy: (section 3.1.2)
///
/// 1. Divide the colours into two palette-sized disjoint sets. (We will use even and odd indices.)
/// 2. Make sure that each tile only has colours from either set (never both!),
///    and that these “proto-palettes” are fed to the packing algorithm alternatingly.
///
/// The image can be displayed using just the two sets, but the greediness of these algorithms
/// makes them generate N palettes composed of one “proto-palette” from each set.
fn defeat_any_fit(palette_size: usize) -> (Vec<Vec<u8>>, u8) {
    assert_eq!(
        palette_size % 2,
        0,
        "Palette size must be even for this strategy!"
    );
    let nb_colors_per_tile = palette_size / 2;
    let nb_colors =
        u8::try_from(palette_size * 2).expect("Only color indices up to 256 are supported!");

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

/// Strategy: (section 3.2.2)
///
/// 0. Let `N = palette_size`, for conciseness, and `A = 0..N` the input alphabet.
/// 1. Construct N tiles of (N-1) colours each. (= the (N-1)-combinations of A).
/// 2. For each (non-overlapping) pair of those tiles, take their intersection (should have size N-2), add two “locking” colours (N and N+1), and do the same as step 1.
/// 3. Make sure each tile uses one proto-palette from step 1, then one from step 2 (never using both A and B), etc.
/// 4. If N > 2, emit the remaining tiles that take
///
/// The image can be displayed using one palette containing all of A, and one palette containing each of the generated `intersection`s with N and N+1 added.
fn defeat_best_fusion(palette_size: usize) -> (Vec<Vec<u8>>, u8) {
    assert_eq!(
        palette_size % 2,
        0,
        "Palette size must be even for this strategy!"
    );
    let nb_colors =
        u8::try_from(palette_size + 2).expect("Only color indices up to 256 are supported!");

    let t0 = combination::combine::index(palette_size, palette_size - 1); // Note that `t0.len() == palette_size`.
    debug_assert_eq!(t0.len(), palette_size); // Guaranteed by maths.
    let a = nb_colors - 2;
    let b = nb_colors - 1;

    let mut proto_palettes =
        Vec::with_capacity(palette_size * 2 + (palette_size / 2) * (palette_size - 2));
    for i in 0..palette_size / 2 {
        let first = &t0[i * 2];
        let second = &t0[i * 2 + 1];

        let intersection: Vec<_> = first
            .iter()
            .filter_map(|t| second.contains(t).then_some(*t as u8))
            .collect();
        debug_assert_eq!(intersection.len(), palette_size - 2); // We will add 1 extra to end up at `palette_size - 1`.

        fn u8ify(slice: &[usize]) -> Vec<u8> {
            slice.iter().map(|&idx| idx as u8).collect()
        }
        let lock = |lock_color| {
            let mut locked = intersection.clone();
            locked.push(lock_color);
            locked
        };
        // These two will go into one palette, and fill it up.
        proto_palettes.push(u8ify(first));
        proto_palettes.push(lock(a));
        // These two will go into another one, and fill it up.
        proto_palettes.push(u8ify(second));
        proto_palettes.push(lock(b));

        // Normally this would be done in a separate loop after this one complete, but honestly we can do both at the same time, and it saves a bit of computation.
        let rest = combination::combine::from_vec_at(&intersection, palette_size - 3); // We will add 2 extra to end up at `palette_size - 1`.
        debug_assert_eq!(rest.len(), palette_size - 2); // Guaranteed by maths.

        // This entire `extend` will only generate a single extra palette.
        proto_palettes.extend(rest.into_iter().map(|mut subpal| {
            subpal.push(a);
            subpal.push(b);
            subpal
        }));
    }

    debug_assert_eq!(
        proto_palettes.len(),
        palette_size * 2 + (palette_size / 2) * (palette_size - 2)
    );
    (proto_palettes, nb_colors)
}

/// Generates tiles that contain the specified colours and none else.
fn gen_image(tile_size: usize, nb_colors: u8, tile_colors: Vec<Vec<u8>>, path: &Path) {
    let palette = (0..nb_colors).map(|i| {
        let (red, green, blue) = Srgb::from_color(Hsl::new_srgb(
            f32::from(i / 2) / f32::from(nb_colors / 2) * 360.,
            1.0,                                  // Max saturation.
            if i % 2 == 0 { 0.25 } else { 0.75 }, // Alternate between darker and brighter colours.
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
