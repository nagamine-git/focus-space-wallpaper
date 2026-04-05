# 星雲生成モジュール仕様書 (nebula)

## 1. 概要

CMB (宇宙マイクロ波背景放射) のパワースペクトルに基づく物理インスパイアドなノイズ生成で、リアルな星雲画像を生成するモジュール。

## 2. ノイズフィールド (`noise_field.rs`)

### 2.1 物理的背景

宇宙の大規模構造はCMBのパワースペクトルに従う:

```
P(k) ∝ k^(n_s - 1)
```

- `k`: 波数 (空間周波数)
- `n_s`: スペクトル指数 ≈ 0.965 (Planck 2018観測値)

`n_s < 1` は大きなスケールにわずかに多くのパワーがあることを意味し、宇宙の大規模構造 (銀河フィラメント、ボイド) を生む。

### 2.2 FBMパラメータによるCMBパワースペクトルの近似

Fractal Brownian Motion (FBM) では各オクターブの振幅が `persistence^octave` で減衰する。これは周波数空間でのパワースペクトルに対応する:

```
FBM amplitude at octave n = persistence^n
FBM frequency at octave n = lacunarity^n

→ P(k) ∝ k^(log(persistence) / log(lacunarity))
```

CMBの `P(k) ∝ k^(n_s - 1) = k^(-0.035)` を近似するには:

```
log(persistence) / log(lacunarity) = n_s - 1 = -0.035
lacunarity = 2.0 の場合:
persistence = 2^(-0.035) ≈ 0.976
```

ただし、視覚的な美しさのために、実際にはやや強い減衰 (persistence ≈ 0.487) を使用し、小スケール構造も適度に表現する。これは `n_s ≈ 0.965` の「精神」を保ちつつ、装飾的な意味でのFBM標準パラメータに近い値。

### 2.3 3レイヤー構成

#### レイヤー1: 密度場 (Density Field)

星雲のメインとなるガス分布を表現する。

```rust
noise::Fbm<noise::Perlin> {
    seed: user_seed,
    octaves: 8,
    frequency: 1.0,      // ベース周波数
    lacunarity: 2.0,      // オクターブ間の周波数倍率
    persistence: 0.487,   // オクターブ間の振幅減衰
}
```

- 入力: 正規化座標 `(nx, ny)` ∈ [-1, 1]
- 出力: [-1, 1] → [0, 1] に正規化

#### レイヤー2: 乱流場 (Turbulence Field)

フィラメント構造やガスの渦を表現する。

```rust
noise::Fbm<noise::SuperSimplex> {
    seed: user_seed + 1,
    octaves: 6,
    frequency: 2.0,       // より高い周波数でディテール重視
    lacunarity: 2.2,
    persistence: 0.45,
}
```

- 密度場より高周波で、ガスの細かな構造を追加

#### レイヤー3: 放射場 (Emission Field)

イーグル星雲の「創造の柱」のような発光リッジ構造を表現する。

```rust
noise::RidgedMulti<noise::Perlin> {
    seed: user_seed + 2,
    octaves: 5,
    frequency: 1.5,
    lacunarity: 2.0,
    attenuation: 2.0,     // リッジの鋭さ
}
```

- `RidgedMulti` は `|noise|` を計算し、鋭い峰 (リッジ) を生成する
- 明るい発光領域として使用

### 2.4 レイヤー合成

```rust
fn composite(density: f64, turbulence: f64, emission: f64) -> f64 {
    let raw = density * 0.5 + turbulence * 0.3 + emission * 0.2;
    raw.clamp(0.0, 1.0)
}
```

### 2.5 ビネット効果

視覚的な深み表現のために、画像端部を暗くするガウシアンビネットを適用:

```rust
fn vignette(x: f64, y: f64, strength: f64) -> f64 {
    // (x, y) は中心 (0, 0) からの正規化座標
    let r2 = x * x + y * y;
    (-r2 * strength).exp()
}
```

- `strength`: ビネットの強さ (デフォルト: 1.5)
- 中心部 `r=0` で 1.0、端部で急速に減衰

## 3. カラーマッピング (`colormap.rs`)

### 3.1 色彩設計の根拠

| 色域 | 心理的効果 | 用途 |
|------|-----------|------|
| 深い青 (ネイビー) | ストレス軽減、集中促進、安定感 | 画像のベース色 |
| 中間青 | 開放感、冷静さ | 星雲の主要領域 |
| 青紫 | 創造性、深み | 遷移色として |
| 深い赤/マゼンタ | 覚醒、エネルギー (抑制的に使用) | 高密度領域のアクセント |
| ほぼ黒 | 宇宙の深淵、目の休息 | 低密度領域 |

### 3.2 カラーマップ制御点

5点のHermiteスプライン補間:

| パラメータ t | R | G | B | 色名 |
|-------------|---|---|---|------|
| 0.00 | 5 | 5 | 15 | 深宇宙の黒 |
| 0.30 | 10 | 20 | 60 | ダークネイビー |
| 0.60 | 30 | 50 | 120 | 星雲ブルー |
| 0.80 | 60 | 40 | 100 | ブルーパープル |
| 1.00 | 100 | 30 | 80 | ディープマゼンタ |

### 3.3 Hermiteスプライン補間

各RGB成分を独立にHermite補間:

```rust
fn hermite_interpolate(t: f32, points: &[(f32, f32)]) -> f32 {
    // t に隣接する2つの制御点を見つけ、
    // Hermite基底関数で補間する
    // タンジェントは隣接セグメントの差分から自動推定 (Catmull-Rom)
}
```

### 3.4 多バンドカラーマッピング

密度値だけでなく、3レイヤーの情報を使ってカラーを決定:

```rust
fn nebula_color(density: f32, emission: f32, turbulence: f32) -> [u8; 3] {
    // 1. ベース色: density → Hermiteスプライン補間
    let base = interpolate_colormap(density);

    // 2. 放射ブースト: emission が高い領域は明るさを加算
    let luminosity_boost = emission * 0.3;

    // 3. 乱流によるヒューシフト: turbulence が高い領域は青緑にシフト
    let hue_shift = turbulence * 0.1;

    // 4. 合成してHDRトーンマッピング
    apply_tonemapping(base, luminosity_boost, hue_shift)
}
```

### 3.5 HDRトーンマッピング

Reinhard風のトーンマッピングで自然な明暗:

```rust
fn tonemap(raw: f32, exposure: f32) -> f32 {
    1.0 - (-exposure * raw).exp()
}
```

- `exposure`: 露出パラメータ (デフォルト: 1.5)
- 低い値 → 暗く落ち着いた画像
- 高い値 → 明るく鮮やかな画像

## 4. 星フィールド (`stars.rs`)

### 4.1 星の配置

```rust
struct Star {
    x: u32,            // ピクセルX座標
    y: u32,            // ピクセルY座標
    brightness: f32,   // 0.0 - 1.0
    radius: u32,       // グローカーネル半径 (0-3)
}
```

- 星の数: 2000-5000個 (4K解像度の場合、設定可能)
- 配置: 一様ランダム分布 (ChaCha RNG使用)

### 4.2 明るさのべき乗則分布

実際の恒星の等級分布を模倣:

```rust
fn star_brightness(rng: &mut ChaChaRng) -> f32 {
    // べき乗則: 暗い星が多く、明るい星は少ない
    let u: f32 = rng.gen();       // [0, 1)
    let alpha = 2.5;              // べき乗指数 (ザルツバーグ光度関数に近似)
    u.powf(alpha)                 // 0に近い値が多い分布
}
```

### 4.3 ガウシアングロー

明るい星 (brightness > 0.7) にはグローを追加:

```rust
fn star_glow_kernel(radius: u32) -> Vec<Vec<f32>> {
    // ガウシアンカーネル (2*radius+1) x (2*radius+1)
    // σ = radius / 2.0
}
```

| brightness | radius | 効果 |
|-----------|--------|------|
| < 0.3 | 0 | 1ピクセルの点 |
| 0.3 - 0.7 | 1 | 3x3グロー |
| 0.7 - 0.9 | 2 | 5x5グロー |
| > 0.9 | 3 | 7x7グロー |

### 4.4 星雲によるオクルージョン

星雲密度が高い領域では星が見えにくくなる:

```rust
fn star_visibility(star_brightness: f32, nebula_density: f32) -> f32 {
    let occlusion = 1.0 - nebula_density.powf(0.5);
    (star_brightness * occlusion).max(0.0)
}
```

### 4.5 星のレンダリング

星の色は白～青白:

```rust
fn star_color(brightness: f32) -> [u8; 3] {
    let b = (brightness * 255.0) as u8;
    // 青白い光: R,G をわずかに下げる
    [(b as f32 * 0.9) as u8, (b as f32 * 0.95) as u8, b]
}
```

## 5. 画像生成オーケストレーター (`generator.rs`)

### 5.1 生成パイプライン

```
1. シード初期化 (指定 or ランダム)
2. 3つのノイズフィールド構築
3. rayon par_iter で全ピクセルを並列計算:
   a. 座標を [-1, 1] に正規化
   b. 3レイヤーのノイズ値取得
   c. レイヤー合成
   d. ビネット適用
   e. カラーマッピング
4. 星フィールド生成・オーバーレイ
5. PNG出力
```

### 5.2 座標の正規化

```rust
fn normalize_coords(x: u32, y: u32, width: u32, height: u32) -> (f64, f64) {
    let aspect = width as f64 / height as f64;
    let nx = (x as f64 / width as f64 * 2.0 - 1.0) * aspect;
    let ny = y as f64 / height as f64 * 2.0 - 1.0;
    (nx, ny)
}
```

- アスペクト比を考慮し、歪みを防止

### 5.3 並列化戦略

```rust
let pixels: Vec<u8> = (0..width * height)
    .into_par_iter()
    .flat_map(|idx| {
        let x = idx % width;
        let y = idx / width;
        // ... ノイズ計算 + カラーマッピング
        [r, g, b, 255u8]
    })
    .collect();
```

### 5.4 出力形式

- フォーマット: PNG (ロスレス)
- カラースペース: sRGB
- ビット深度: 8bit/チャンネル (RGBA)
- 4K (3840x2160) で約15-30MB

## 6. 画像ブレンド (`blend.rs`)

### 6.1 アルファブレンド

2枚の画像間をスムーズに遷移:

```rust
fn blend(current: &RgbaImage, next: &RgbaImage, t: f32) -> RgbaImage {
    // t: 0.0 = current のみ、1.0 = next のみ
    // pixel = current * (1 - t) + next * t
}
```

### 6.2 イージング関数

```rust
fn smoothstep(t: f32) -> f32 {
    // SmoothStep: 始まりと終わりが滑らか
    t * t * (3.0 - 2.0 * t)
}
```

## 7. 設定パラメータ

```toml
[generation]
width = 3840
height = 2160
octaves = 8
spectral_index = 0.965
star_count = 3000
exposure = 1.5
vignette_strength = 1.5

[colors]
palette = [
    [0.0,  5,   5,  15],
    [0.3,  10,  20, 60],
    [0.6,  30,  50, 120],
    [0.8,  60,  40, 100],
    [1.0,  100, 30, 80],
]
```
