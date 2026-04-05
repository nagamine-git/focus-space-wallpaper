/// Catmull-Rom スプラインによるカラーマップ補間
pub struct Colormap {
    points: Vec<[f32; 4]>, // [t, r, g, b]
}

impl Colormap {
    pub fn new(points: Vec<[f32; 4]>) -> Self {
        let mut pts = points;
        pts.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        Self { points: pts }
    }

    /// t ∈ [0, 1] に対応する [r, g, b] を返す (各値 0-255)
    pub fn sample(&self, t: f32) -> [u8; 3] {
        let pts = &self.points;
        if pts.len() < 2 {
            return [0, 0, 0];
        }

        // t が範囲外の場合はクランプ
        let t = t.clamp(pts.first().unwrap()[0], pts.last().unwrap()[0]);

        // 隣接するセグメントを探す
        let idx = pts
            .windows(2)
            .position(|w| t >= w[0][0] && t <= w[1][0])
            .unwrap_or(pts.len() - 2);

        let p0 = &pts[idx.saturating_sub(1)];
        let p1 = &pts[idx];
        let p2 = &pts[(idx + 1).min(pts.len() - 1)];
        let p3 = &pts[(idx + 2).min(pts.len() - 1)];

        // セグメント内の局所パラメータ
        let seg_t0 = p1[0];
        let seg_t1 = p2[0];
        let local_t = if (seg_t1 - seg_t0).abs() < 1e-6 {
            0.0
        } else {
            (t - seg_t0) / (seg_t1 - seg_t0)
        };

        // Catmull-Rom 基底関数
        let r = catmull_rom(local_t, p0[1], p1[1], p2[1], p3[1]);
        let g = catmull_rom(local_t, p0[2], p1[2], p2[2], p3[2]);
        let b = catmull_rom(local_t, p0[3], p1[3], p2[3], p3[3]);

        [
            r.clamp(0.0, 255.0) as u8,
            g.clamp(0.0, 255.0) as u8,
            b.clamp(0.0, 255.0) as u8,
        ]
    }
}

fn catmull_rom(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// HDR トーンマッピング: [0, ∞) → [0, 1)
pub fn tonemap(raw: f32, exposure: f32) -> f32 {
    1.0 - (-exposure * raw).exp()
}

/// 3レイヤーの値からピクセルカラーを決定する
/// JWST/Hubble スタイル: 背景(青黒宇宙空間) + ガス雲(暖色橙茶金) の分離ブレンド
pub fn nebula_color(
    composite: f32,
    emission: f32,
    turbulence: f32,
    colormap: &Colormap,
    exposure: f32,
) -> [u8; 3] {
    // =====================================================
    // 背景色: 深宇宙の青黒
    // =====================================================
    const BG_R: f32 = 4.0;
    const BG_G: f32 = 7.0;
    const BG_B: f32 = 28.0;

    // =====================================================
    // ガス雲マスク: シグモイドで崖状の鋭いエッジを生成
    // =====================================================
    // composite が threshold を超えた場所にガス雲が出現
    let threshold = 0.42_f32;
    let sharpness = 12.0_f32;
    let gas_mask = 1.0_f32 / (1.0 + (-sharpness * (composite - threshold)).exp());

    // =====================================================
    // ガス雲の色 (カラーマップ: 暗い端 → 橙褐 → 金白)
    // =====================================================
    // emission が高い場所はカラーマップの高い方 (輝点: 金白)
    let gas_t = (composite * 0.55 + emission * 0.45).clamp(0.0, 1.0);
    let mapped = tonemap(gas_t, exposure);
    // ガス内部は γ=1.1 程度で自然なグラデーション (潰しすぎない)
    let gas_t_gamma = mapped.powf(1.1);
    let [gr, gg, gb] = colormap.sample(gas_t_gamma);

    // =====================================================
    // 背景 + ガス雲のブレンド
    // =====================================================
    let r = BG_R + (gr as f32 - BG_R) * gas_mask;
    let g = BG_G + (gg as f32 - BG_G) * gas_mask;
    let b = BG_B + (gb as f32 - BG_B) * gas_mask;

    // =====================================================
    // 発光ノット: 恒星形成域のピーク輝点 (白〜橙白)
    // =====================================================
    let emission_hl = ((emission - 0.72).max(0.0) * 3.5 * 70.0 * gas_mask) as u8;

    // turbulence による微細な青緑ティント (希薄ガスの散乱光, O-III)
    // ガス内部のみに適用
    let oiii_tint = ((turbulence - 0.60).max(0.0) * 2.0 * 15.0 * gas_mask) as u8;

    [
        (r.clamp(0.0, 255.0) as u8).saturating_add(emission_hl),
        (g.clamp(0.0, 255.0) as u8).saturating_add(emission_hl / 3).saturating_add(oiii_tint / 2),
        (b.clamp(0.0, 255.0) as u8).saturating_add(oiii_tint),
    ]
}
