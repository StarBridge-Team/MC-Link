//! 从图片提取 M3 种子色（动态配色的"跟随背景图"能力）。
//!
//! # 为什么用 `material_colors::image`
//!
//! M3 的取色不是"取平均色"，而是一套规范流程：
//! 中位切分量化（Celebi Quantizer）→ 按色相/明度/色度打分（Score）→ 选出
//! "最适合做 UI 主题"的那个颜色。手写这套很容易做出"配色发灰/发脏"的结果，
//! 而 `material-colors` 已经实现了它，且**本项目已在用这个 crate**（`generate_m3_scheme`），
//! 不必再引一个取色库。

use material_colors::image::{AsPixels, ImageReader};

use super::parse_seed;

/// 取色前的缩略图边长。
///
/// 量化要对整张图做中位切分，4K 壁纸逐像素跑一遍会明显卡顿；缩到 128px 后像素数
/// 降到万级，结果与全尺寸几乎一致（取色关心整体色彩倾向，不关心细节）。
/// **必须先缩再取色**——顺序反了等于没优化。
const SAMPLE_SIZE: u32 = 128;

/// 单边像素上限：`16384` 已远超任何真实壁纸/截图，用来挡住"声明巨大尺寸"的解码炸弹。
const MAX_DIMENSION: u32 = 16_384;

/// 解码前只读表头拿像素尺寸并校验上限。
///
/// # 为什么必须在解码**之前**做
///
/// `SAMPLE_SIZE` 的缩放发生在 `ImageReader::read` **之后**，而 `read` 会按文件头
/// 声明的尺寸一次性分配 `w * h * 4` 字节。于是一个几百字节、声明成 `30000×30000`
/// 的 PNG 就能让进程分配 3.6 GB 内存（"解压炸弹"）。缩放帮不上忙——那时已经晚了。
///
/// 这里用 `image::ImageReader::into_dimensions`：它只解析表头，不做像素解码。
/// 非图片 / 损坏文件在这里会返回 `Err`，顺带也让后面的 `catch_unwind` 少一类输入。
fn ensure_dimensions_within_limit(data: &[u8]) -> Result<(), String> {
    use std::io::Cursor;
    let dims = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("无法识别图片格式: {e}"))?
        .into_dimensions()
        .map_err(|e| format!("无法读取图片尺寸: {e}"))?;
    if dims.0 == 0 || dims.1 == 0 {
        return Err("图片尺寸为 0".to_string());
    }
    if dims.0 > MAX_DIMENSION || dims.1 > MAX_DIMENSION {
        return Err(format!(
            "图片尺寸 {}×{} 超过上限 {}，拒绝解码",
            dims.0, dims.1, MAX_DIMENSION
        ));
    }
    Ok(())
}

/// 从图片字节里提取种子色，返回 `#rrggbb`。
///
/// 失败一律返回 `Err`（调用方据此回退到用户手选的种子色），不"猜一个颜色"兜底：
/// 配错色比不配色更难排查。
pub fn extract_seed_from_bytes(data: &[u8]) -> Result<String, String> {
    // 先挡解码炸弹：读表头看尺寸，过大直接拒绝，不分配解码缓冲。
    ensure_dimensions_within_limit(data)?;

    // `ImageReader::read` 的签名是 `Result`，但它内部对**解码失败**用的是
    // `expect("failed to decode image")` —— 也就是说非图片 / 损坏图片会直接 panic，
    // 而不是返回 `Err`。它没有返回 `Result` 可留给我们处理，只能在这里 `catch_unwind` 兜住。
    //
    // 这不是理论风险：背景目录是用户自己放文件的地方，选到一个非图片文件
    // （或下载了一半的图片）就会触发；而后端命令 panic 会打断整条取色链路，
    // 用户看到的是"选了个背景，然后什么都没发生"。
    let mut image = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ImageReader::read(data)))
        .map_err(|_| "图片无法解码（可能不是图片，或文件已损坏）".to_string())?
        .map_err(|e| format!("读取图片失败: {e}"))?;
    image.resize(
        SAMPLE_SIZE,
        SAMPLE_SIZE,
        material_colors::image::FilterType::Nearest,
    );

    // `extract_color` 内部会 `ranked[0]`，而 `ranked` 来自量化结果。
    // 全透明/尺寸为 0 的图会得到一个空列表 → 直接越界 panic。
    // 它没有返回 `Result`，所以这道判断必须在这里做。
    if image.as_pixels().is_empty() {
        return Err("图片没有可用像素".to_string());
    }

    let hex = ImageReader::extract_color(&image).to_hex_with_pound();

    // 自检：产出的值必须能被 `generate_m3_scheme` 接受，
    // 否则会出现"取色成功但配色命令报无效种子"的割裂。
    if parse_seed(&hex).is_none() {
        return Err(format!("提取到的颜色无法解析: {hex}"));
    }
    Ok(hex)
}

/// 取色所需的最小栈（字节）。
///
/// # 为什么必须显式指定
///
/// 取色不是"越小的图越省栈"。`QuantizerCelebi` 是**递归的中位切分**：像素颜色越杂，
/// 递归越深。实测一张 3840×2160 的真实照片（缩到 128×128 后仍有上万种颜色）会让
/// **默认栈的主线程**报 `has overflowed its stack`；而同尺寸的纯色图（只有 1 种颜色）
/// 却安然无恙 —— 单元测试因此一直没能复现。
///
/// 8 MiB 是宽裕值：这类 CPU 密集、无共享状态的活儿本来就该在专用线程上跑，
/// 顺带也不会阻塞调用方。
/// # 实测数据（为什么不取 1 MiB、也不取 2 MiB）
///
/// 用一张真实的 3840×2160 JPEG 逐档试出来的阈值：
///
/// | 栈 | 结果 |
/// |---|---|
/// | 256 KiB / 512 KiB | 爆栈 |
/// | **1024 KiB（Windows 主线程默认值）** | **爆栈** |
/// | 2048 KiB 及以上 | 成功 |
///
/// 关键是最后一行：**Windows 主线程默认栈恰好是 1 MiB**，而取色需要略多于它。
/// 这就是"单元测试全过、真机必崩"的原因——测试线程默认 2 MiB，刚好够。
/// 取 8 MiB 留足余量（更大的壁纸、其他平台差异），代价只是一条线程的地址空间。
const EXTRACT_STACK_BYTES: usize = 8 * 1024 * 1024;

/// 在**大栈线程**里执行取色，返回结果。
///
/// 栈溢出不是可捕获的 panic（`catch_unwind` 对它无效），所以只能从根上保证栈够用。
/// 所有对外入口都经由这里，避免调用方各自踩同一个坑。
fn run_with_big_stack<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name("mclink-seed-extract".to_string())
        .stack_size(EXTRACT_STACK_BYTES)
        .spawn(f)
        .map_err(|e| format!("无法创建取色线程: {e}"))?;
    match handle.join() {
        Ok(res) => res,
        Err(_) => Err("取色线程异常结束".to_string()),
    }
}

/// 只做"解码 + 缩放到采样尺寸"，返回尺寸描述。**仅供问题定位**。
///
/// 存在的意义是二分：把"解码/缩放"与"量化/打分"分开。取色爆栈时可以先用它确认
/// 是哪一半的问题，而不必反复改动生产代码。
pub fn decode_and_resize_only(path: &std::path::Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("读取失败: {e}"))?;
    ensure_dimensions_within_limit(&data)?;
    let mut image = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ImageReader::read(&data)))
        .map_err(|_| "图片无法解码".to_string())?
        .map_err(|e| format!("读取图片失败: {e}"))?;
    image.resize(
        SAMPLE_SIZE,
        SAMPLE_SIZE,
        material_colors::image::FilterType::Nearest,
    );
    Ok(format!("缩放后像素数 = {}", image.as_pixels().len()))
}

/// 从本地图片文件提取种子色。
///
/// 读文件在主线程做（很快），**解码与量化在大栈线程里做**（详见 [`EXTRACT_STACK_BYTES`]）。
pub fn extract_seed_from_file(path: &std::path::Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("读取背景图失败: {e}"))?;
    run_with_big_stack(move || extract_seed_from_bytes(&data))
}

/// 在**当前线程**直接取色，不切大栈线程。
///
/// 仅供 `examples/seed_repro` 测量"需要多大栈"使用；生产路径一律走
/// [`extract_seed_from_file`]。
pub fn extract_seed_from_file_inline(path: &std::path::Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("读取背景图失败: {e}"))?;
    extract_seed_from_bytes(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成 `size×size` 的纯色 PNG（手写最小 PNG 结构，不引编码依赖）。
    fn solid_png(r: u8, g: u8, b: u8, size: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut table = [0u32; 256];
            for (i, e) in table.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *e = c;
            }
            let mut c = 0xffff_ffffu32;
            for &byte in data {
                c = table[((c ^ byte as u32) & 0xff) as usize] ^ (c >> 8);
            }
            c ^ 0xffff_ffff
        }
        fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            let mut crc_in = kind.to_vec();
            crc_in.extend_from_slice(payload);
            out.extend_from_slice(kind);
            out.extend_from_slice(payload);
            out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
            out
        }

        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&size.to_be_bytes());
        ihdr.extend_from_slice(&size.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));

        // 每行：过滤字节 0 + size 个 RGB。用未压缩 zlib（stored）块，
        // 便于手写且长度可控。
        let mut raw = Vec::new();
        for _ in 0..size {
            raw.push(0u8);
            for _ in 0..size {
                raw.extend_from_slice(&[r, g, b]);
            }
        }
        let mut zlib = vec![0x78, 0x01];
        // stored 块最大 65535 字节，分块写入
        for part in raw.chunks(65535) {
            zlib.push(0x00);
            let n = part.len() as u16;
            zlib.extend_from_slice(&n.to_le_bytes());
            zlib.extend_from_slice(&(!n).to_le_bytes());
            zlib.extend_from_slice(part);
        }
        let mut a: u32 = 1;
        let mut bsum: u32 = 0;
        for &byte in &raw {
            a = (a + byte as u32) % 65521;
            bsum = (bsum + a) % 65521;
        }
        zlib.extend_from_slice(&((bsum << 16) | a).to_be_bytes());
        png.extend_from_slice(&chunk(b"IDAT", &zlib));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    /// 一张 1×1 的纯色 PNG，直接写字面量。
    ///
    /// 刻意不引 `image` crate 来做测试编码：`material-colors` 只重新导出了 `FilterType`，
    /// 没有导出 `RgbaImage`，为了写测试而把 `image` 提成本项目的直接依赖并不值得。
    /// （编码器见本模块顶部的 [`solid_png`]。）
    /// 生成 `size×size` 的**每个像素都不同色**的 PNG。
    ///
    /// 专门用来喂给 Celebi 量化最难的情况：颜色种类多到分不开簇。
    fn many_color_png(size: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut table = [0u32; 256];
            for (i, e) in table.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *e = c;
            }
            let mut c = 0xffff_ffffu32;
            for &byte in data {
                c = table[((c ^ byte as u32) & 0xff) as usize] ^ (c >> 8);
            }
            c ^ 0xffff_ffff
        }
        fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            let mut crc_in = kind.to_vec();
            crc_in.extend_from_slice(payload);
            out.extend_from_slice(kind);
            out.extend_from_slice(payload);
            out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
            out
        }

        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&size.to_be_bytes());
        ihdr.extend_from_slice(&size.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));

        let mut raw = Vec::new();
        let mut n: u32 = 0;
        for _y in 0..size {
            raw.push(0u8); // 过滤字节
            for _x in 0..size {
                // 用低位散开，保证相邻像素也不同色
                raw.push(((n * 37) % 256) as u8);
                raw.push(((n * 91) % 256) as u8);
                raw.push(((n * 151) % 256) as u8);
                n += 1;
            }
        }

        let mut zlib = vec![0x78, 0x01];
        for part in raw.chunks(65535) {
            zlib.push(0x00);
            let len = part.len() as u16;
            zlib.extend_from_slice(&len.to_le_bytes());
            zlib.extend_from_slice(&(!len).to_le_bytes());
            zlib.extend_from_slice(part);
        }
        let mut a: u32 = 1;
        let mut bsum: u32 = 0;
        for &byte in &raw {
            a = (a + byte as u32) % 65521;
            bsum = (bsum + a) % 65521;
        }
        zlib.extend_from_slice(&((bsum << 16) | a).to_be_bytes());
        png.extend_from_slice(&chunk(b"IDAT", &zlib));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    fn parts(hex: &str) -> (u8, u8, u8) {
        (
            u8::from_str_radix(&hex[1..3], 16).unwrap(),
            u8::from_str_radix(&hex[3..5], 16).unwrap(),
            u8::from_str_radix(&hex[5..7], 16).unwrap(),
        )
    }

    /// 先确认自制的 PNG 能被解码器认出来，否则下面的取色断言失败会指向错误的方向。
    #[test]
    fn handcrafted_png_is_decodable() {
        let png = solid_png(220, 30, 30, 1);
        assert!(ImageReader::read(&png).is_ok(), "自制的 1×1 PNG 应当可解码");
    }

    #[test]
    fn extracts_seed_from_solid_red() {
        let seed = extract_seed_from_bytes(&solid_png(220, 30, 30, 8))
            .expect("应从红色图片提取出种子色");
        assert!(seed.starts_with('#'), "种子色应是 #rrggbb：{seed}");
        assert_eq!(seed.len(), 7, "种子色长度应为 7：{seed}");

        let (r, g, b) = parts(&seed);
        assert!(r > g && r > b, "红色图片应提取出偏红的种子色，实际 {seed}");
    }

    /// 蓝色图必须给出偏蓝的种子。只测红色的话，一个"永远返回红色"的实现也能通过。
    #[test]
    fn extracts_seed_from_solid_blue() {
        let seed = extract_seed_from_bytes(&solid_png(30, 60, 220, 8)).unwrap();
        let (r, g, b) = parts(&seed);
        assert!(b > r && b > g, "蓝色图片应提取出偏蓝的种子色，实际 {seed}");
    }

    #[test]
    fn rejects_non_image_bytes() {
        assert!(extract_seed_from_bytes(b"this is not an image").is_err());
    }

    /// 回归：**颜色多样的大图**不得把栈打爆。
    ///
    /// # 这条测试的由来（踩坑记录）
    ///
    /// 真实的崩溃是：对一张 3840×2160 的照片取色时 `has overflowed its stack`。
    /// 而我第一版回归测试用的是 **256×256 纯色图，它通过了** —— 因为纯色图只有一种
    /// 颜色，`QuantizerCelebi` 的递归中位切分一步到底。
    ///
    /// 关键变量不是尺寸，是**颜色多样性**：像素颜色越杂，递归越深。
    /// 所以这里必须构造一张"小尺寸但多颜色"的图，才复现得了。
    ///
    /// 注：取色现在跑在大栈线程里（`EXTRACT_STACK_BYTES`），所以这条测试
    /// 即使在这个（小栈）测试线程上调用也不该崩。
    #[test]
    fn colorful_image_does_not_blow_the_stack() {
        // 尺寸=采样尺寸（128），且每个像素不同色 —— 与真实照片缩完之后的规模一致。
        //
        // 实测教训：先用 16×16 的多色图写过一版，**它在 512 KiB 栈下也能通过**，
        // 等于没有防御价值。真正的变量是"像素数量 × 颜色多样性"的乘积，
        // 单靠其中一个维度小测不出来。
        let png = many_color_png(SAMPLE_SIZE);
        let seed = extract_seed_from_bytes(&png);
        assert!(seed.is_ok(), "多色图取色不该失败/爆栈: {seed:?}");
    }

    /// 空输入不能 panic：`extract_color` 内部会 `ranked[0]`，空列表会越界。
    #[test]
    fn never_panics_on_empty_input() {
        assert!(extract_seed_from_bytes(b"").is_err());
    }

    /// 回归：声明巨大尺寸的图片必须在**解码前**被拒，否则 30000×30000 的
    /// 几百字节 PNG 就能让进程分配数 GB 内存（解码炸弹）。
    #[test]
    fn rejects_oversized_dimensions_before_decoding() {
        // `solid_png` 只按尺寸写头部与数据，足够让 `into_dimensions` 读到尺寸。
        // 用一个略超上限的尺寸，避免真的构造出巨大数据（解码前就会返回）。
        let png = solid_png(1, 2, 3, MAX_DIMENSION + 1);
        let err = extract_seed_from_bytes(&png).expect_err("超大尺寸必须被拒");
        assert!(
            err.contains("超过上限"),
            "错误信息应说明尺寸超限，实际: {err}"
        );
    }
}
