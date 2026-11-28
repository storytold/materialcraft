use craft_texture::Image;
use std::f32::consts::PI;

pub struct DisjointSet {
    parent: Vec<u32>,
    rank: Vec<u8>,
}

impl DisjointSet {
    pub fn new(size: usize) -> Self {
        Self { parent: (0..size as u32).collect(), rank: vec![0; size] }
    }

    pub fn find(&mut self, mut x: u32) -> u32 {
        let mut root = x;
        while let Some(&p) = self.parent.get(root as usize) {
            if p == root {
                break;
            }
            root = p;
        }
        while let Some(&p) = self.parent.get(x as usize) {
            if p == root {
                break;
            }
            if let Some(entry) = self.parent.get_mut(x as usize) {
                *entry = root;
            }
            x = p;
        }
        root
    }

    pub fn union(&mut self, a: u32, b: u32) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            let rank_a = self.rank.get(root_a as usize).copied().unwrap_or(0);
            let rank_b = self.rank.get(root_b as usize).copied().unwrap_or(0);
            if rank_a < rank_b {
                if let Some(entry) = self.parent.get_mut(root_a as usize) {
                    *entry = root_b;
                }
            } else {
                if let Some(entry) = self.parent.get_mut(root_b as usize) {
                    *entry = root_a;
                }
                if rank_a == rank_b
                    && let Some(entry) = self.rank.get_mut(root_a as usize)
                {
                    *entry = entry.saturating_add(1);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IslandInfo {
    pub centroid_u: f32,
    pub centroid_v: f32,
    pub bbox_w: f32,
    pub bbox_h: f32,
}

struct IslandAccumulator {
    count: u32,
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
    cos_sum_x: f32,
    sin_sum_x: f32,
    cos_sum_y: f32,
    sin_sum_y: f32,
}

pub fn analyze_mask(mask: &Image, threshold: f32, tile: bool) -> (Vec<u32>, Vec<IslandInfo>) {
    let w = mask.width() as usize;
    let h = mask.height() as usize;
    let total_pixels = w.saturating_mul(h);

    if total_pixels == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut is_fg = vec![false; total_pixels];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if let Some(p) = is_fg.get_mut(idx) {
                *p = mask.get_gray(x as i64, y as i64, false) > threshold;
            }
        }
    }

    let mut dsu = DisjointSet::new(total_pixels);

    // 4-connectivity passes
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !is_fg.get(idx).copied().unwrap_or(false) {
                continue;
            }

            // Right neighbor
            if x + 1 < w {
                let right_idx = idx + 1;
                if is_fg.get(right_idx).copied().unwrap_or(false) {
                    dsu.union(idx as u32, right_idx as u32);
                }
            } else if tile && is_fg.get(y * w).copied().unwrap_or(false) {
                dsu.union(idx as u32, (y * w) as u32);
            }

            // Bottom neighbor
            if y + 1 < h {
                let bottom_idx = idx + w;
                if is_fg.get(bottom_idx).copied().unwrap_or(false) {
                    dsu.union(idx as u32, bottom_idx as u32);
                }
            } else if tile && is_fg.get(x).copied().unwrap_or(false) {
                dsu.union(idx as u32, x as u32);
            }
        }
    }

    // Map root ID to contiguous island index (0..num_islands)
    let mut root_to_island: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut labels = vec![0u32; total_pixels];
    let mut island_stats: Vec<IslandAccumulator> = Vec::new();

    let inv_w = 1.0 / w as f32;
    let inv_h = 1.0 / h as f32;
    let two_pi = 2.0 * PI;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !is_fg.get(idx).copied().unwrap_or(false) {
                continue;
            }

            let root = dsu.find(idx as u32);
            let island_id = match root_to_island.get(&root) {
                Some(&id) => id,
                None => {
                    let new_id = root_to_island.len() as u32;
                    root_to_island.insert(root, new_id);
                    island_stats.push(IslandAccumulator {
                        count: 0,
                        min_x: x as u32,
                        max_x: x as u32,
                        min_y: y as u32,
                        max_y: y as u32,
                        cos_sum_x: 0.0,
                        sin_sum_x: 0.0,
                        cos_sum_y: 0.0,
                        sin_sum_y: 0.0,
                    });
                    new_id
                }
            };

            if let Some(lbl) = labels.get_mut(idx) {
                *lbl = island_id + 1; // 1-based, 0 = background
            }

            if let Some(acc) = island_stats.get_mut(island_id as usize) {
                acc.count = acc.count.saturating_add(1);
                acc.min_x = acc.min_x.min(x as u32);
                acc.max_x = acc.max_x.max(x as u32);
                acc.min_y = acc.min_y.min(y as u32);
                acc.max_y = acc.max_y.max(y as u32);

                let theta_x = (x as f32 + 0.5) * inv_w * two_pi;
                let theta_y = (y as f32 + 0.5) * inv_h * two_pi;

                acc.cos_sum_x += theta_x.cos();
                acc.sin_sum_x += theta_x.sin();
                acc.cos_sum_y += theta_y.cos();
                acc.sin_sum_y += theta_y.sin();
            }
        }
    }

    let mut islands = Vec::with_capacity(island_stats.len());
    for acc in island_stats {
        if acc.count == 0 {
            continue;
        }

        let mean_theta_x = acc.sin_sum_x.atan2(acc.cos_sum_x).rem_euclid(two_pi);
        let mean_theta_y = acc.sin_sum_y.atan2(acc.cos_sum_y).rem_euclid(two_pi);

        let centroid_u = mean_theta_x / two_pi;
        let centroid_v = mean_theta_y / two_pi;

        let bbox_w = ((acc.max_x - acc.min_x + 1) as f32 * inv_w).clamp(0.0, 1.0);
        let bbox_h = ((acc.max_y - acc.min_y + 1) as f32 * inv_h).clamp(0.0, 1.0);

        islands.push(IslandInfo { centroid_u, centroid_v, bbox_w, bbox_h });
    }

    (labels, islands)
}
