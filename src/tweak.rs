use crate::pck;
use anyhow::{anyhow, Context, Result};
use rust_embed::RustEmbed;
use std::fs::OpenOptions;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

/// 读取 replace.toml，解析资源映射并从内嵌 assets 取出数据
fn load_replacements() -> Result<Vec<(String, Vec<u8>)>> {
    let config_str = include_str!("../assets/replace.toml");
    let table: toml::value::Table =
        toml::from_str(config_str).context("解析 replace.toml 失败")?;

    let mut replacements = Vec::with_capacity(table.len());
    for (res_path, asset_value) in table {
        let asset_path = asset_value
            .as_str()
            .ok_or_else(|| anyhow!("replace.toml 中的值必须是字符串: {}", res_path))?;

        // 兼容 "../assets/xxx" 或 "xxx" 两种写法
        let embedded_key = asset_path
            .trim_start_matches("../assets/")
            .trim_start_matches("./");

        let asset = Assets::get(embedded_key)
            .ok_or_else(|| anyhow!("嵌入资源缺失: {}", asset_path))?;

        replacements.push((res_path, asset.data.to_vec()));
    }

    Ok(replacements)
}

/// 修改指定路径 PCK 文件并应用预置替换
pub fn tweak_game_gde(file_path: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(file_path)
        .with_context(|| format!("无法打开文件: {}", file_path))?;

    let (_header, index) = pck::read_header_and_index(&mut file)?;

    let replacements_owned = load_replacements()?;
    let replacements: Vec<(&str, &[u8])> = replacements_owned
        .iter()
        .map(|(path, data)| (path.as_str(), data.as_slice()))
        .collect();

    pck::replace_files_in_pck(&mut file, &index, replacements)?;

    Ok(())
}
