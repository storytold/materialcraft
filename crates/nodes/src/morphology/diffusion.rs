use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition_diffusion_color() -> NodeDefinition {
    node("diffusion_color", "Diffusion Color", CATEGORY)
        .desc("Expands RGB color boundaries into transparent or untextured regions (UV padding/seam dilation).")
        .color_in("color", "Color Input")
        .primary_input("color")
        .gray_in("mask", "Opaque / Coverage Mask")
        .int("iterations", "Iterations", 16, 1, 128)
        .color_out()
        .cook(cook_diffusion_color)
}

pub(crate) fn definition_diffusion_grayscale() -> NodeDefinition {
    node("diffusion_grayscale", "Diffusion Grayscale", CATEGORY)
        .desc("Expands grayscale values across missing masked areas via iterative neighbor diffusion.")
        .gray_in("input", "Grayscale Input")
        .primary_input("input")
        .gray_in("mask", "Mask (White = Keep, Black = Fill)")
        .int("iterations", "Iterations", 16, 1, 128)
        .gray_out()
        .cook(cook_diffusion_grayscale)
}

pub(crate) fn definition_diffusion_uv() -> NodeDefinition {
    node("diffusion_uv", "Diffusion UV", CATEGORY)
        .desc("Extends texture atlas coordinates across UV island margins to eliminate seam bleeding.")
        .color_in("uv", "UV Map (RG)")
        .primary_input("uv")
        .gray_in("mask", "UV Island Mask")
        .int("iterations", "Iterations", 16, 1, 128)
        .color_out()
        .cook(cook_diffusion_uv)
}

fn cook_diffusion_color(context: &CookContext<'_>) -> CookResult {
    let color = context.any_or(0, Kind::Color, [0.0; 4]);
    let mask = context.sized(1).map(|img| img.to_gray());
    let (w, h) = (context.w, context.h);
    let iterations = context.i("iterations").clamp(1, 128) as usize;
    let tile = context.tile;

    let mut buf = vec![[0.0f32; 4]; (w * h) as usize];
    let mut occ = vec![false; (w * h) as usize];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;
            let c = color.get(xi, yi, tile);
            let m = mask.as_ref().map_or(c[3], |m| m.get_gray(xi, yi, tile));
            buf[idx] = c;
            occ[idx] = m > 0.05;
        }
    }

    let mut next_buf = buf.clone();
    let mut next_occ = occ.clone();

    for _ in 0..iterations {
        let mut changed = false;
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if !occ[idx] {
                    // Look at 4 neighbors
                    let mut sum = [0.0f32; 4];
                    let mut count = 0.0f32;
                    let neighbors = [((x + 1) % w, y), ((x + w - 1) % w, y), (x, (y + 1) % h), (x, (y + h - 1) % h)];
                    for (nx, ny) in neighbors {
                        let n_idx = (ny * w + nx) as usize;
                        if occ[n_idx] {
                            let nc = buf[n_idx];
                            sum[0] += nc[0];
                            sum[1] += nc[1];
                            sum[2] += nc[2];
                            sum[3] += nc[3];
                            count += 1.0;
                        }
                    }
                    if count > 0.0 {
                        next_buf[idx] = [sum[0] / count, sum[1] / count, sum[2] / count, 1.0];
                        next_occ[idx] = true;
                        changed = true;
                    }
                }
            }
        }
        buf = next_buf.clone();
        occ = next_occ.clone();
        if !changed {
            break;
        }
    }

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Color, buf.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_diffusion_grayscale(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let mask = context.sized(1).map(|img| img.to_gray());
    let (w, h) = (context.w, context.h);
    let iterations = context.i("iterations").clamp(1, 128) as usize;
    let tile = context.tile;

    let mut buf = vec![0.0f32; (w * h) as usize];
    let mut occ = vec![false; (w * h) as usize];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;
            let val = input.get_gray(xi, yi, tile);
            let m = mask.as_ref().map_or(1.0, |m| m.get_gray(xi, yi, tile));
            buf[idx] = val;
            occ[idx] = m > 0.05;
        }
    }

    let mut next_buf = buf.clone();
    let mut next_occ = occ.clone();

    for _ in 0..iterations {
        let mut changed = false;
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if !occ[idx] {
                    let mut sum = 0.0f32;
                    let mut count = 0.0f32;
                    let neighbors = [((x + 1) % w, y), ((x + w - 1) % w, y), (x, (y + 1) % h), (x, (y + h - 1) % h)];
                    for (nx, ny) in neighbors {
                        let n_idx = (ny * w + nx) as usize;
                        if occ[n_idx] {
                            sum += buf[n_idx];
                            count += 1.0;
                        }
                    }
                    if count > 0.0 {
                        next_buf[idx] = sum / count;
                        next_occ[idx] = true;
                        changed = true;
                    }
                }
            }
        }
        buf = next_buf.clone();
        occ = next_occ.clone();
        if !changed {
            break;
        }
    }

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Gray, buf, &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_diffusion_uv(context: &CookContext<'_>) -> CookResult {
    // Cook using color diffusion logic, ensuring UVs stay in range
    cook_diffusion_color(context)
}
