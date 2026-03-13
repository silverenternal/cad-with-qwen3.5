//! 提示词模板模块 - 运行时加载模板

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn, error};
use crate::server::types::DrawingType;

/// 提示词模板配置
#[derive(Debug, Clone, Deserialize)]
pub struct PromptTemplateConfig {
    /// 各图纸类型的模板
    #[serde(flatten)]
    pub templates: HashMap<String, TemplateSection>,
    /// 全局配置
    #[serde(default)]
    pub global: GlobalConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSection {
    #[serde(default)]
    pub system: String,
    #[serde(default)]
    pub examples: Option<ExamplesConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExamplesConfig {
    pub file: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default = "default_true")]
    pub include_examples: bool,
    #[serde(default)]
    pub use_short_prompt: bool,
    #[serde(default = "default_drawing_type")]
    pub default_drawing_type: String,
}

fn default_true() -> bool { true }
fn default_drawing_type() -> String { "building_plan".to_string() }

/// 提示词模板构建器
pub struct PromptTemplate {
    config: Option<PromptTemplateConfig>,
    drawing_type: DrawingType,
}

impl PromptTemplate {
    /// 从文件加载模板配置
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: PromptTemplateConfig = toml::from_str(&content)
            .unwrap_or_else(|e| {
                error!("Failed to parse prompt template: {}, using built-in templates", e);
                PromptTemplateConfig {
                    templates: HashMap::new(),
                    global: GlobalConfig {
                        include_examples: true,
                        use_short_prompt: false,
                        default_drawing_type: "building_plan".to_string(),
                    },
                }
            });

        Ok(Self {
            config: Some(config),
            drawing_type: DrawingType::BuildingPlan, // 默认值，后续可设置
        })
    }

    /// 使用内置模板（硬编码 fallback）
    pub fn with_default(drawing_type: DrawingType) -> Self {
        Self {
            config: None,
            drawing_type,
        }
    }

    /// 设置图纸类型
    pub fn with_drawing_type(mut self, drawing_type: DrawingType) -> Self {
        self.drawing_type = drawing_type;
        self
    }

    /// 构建完整的系统提示词
    pub fn build(&self) -> String {
        // 获取图纸类型对应的键名
        let type_key = self.get_type_key();

        // 尝试从配置中获取模板
        if let Some(config) = &self.config {
            if let Some(template) = config.templates.get(&type_key) {
                if !template.system.is_empty() {
                    info!("✓ Using prompt template from prompt.toml for '{}'", type_key);
                    info!("  Template length: {} characters", template.system.len());

                    // 检查是否应该包含示例
                    if config.global.include_examples {
                        if let Some(examples) = &template.examples {
                            info!("  Including examples from: {}", examples.file);
                            // 加载示例文件并追加到模板
                            if let Some(examples_content) = self.load_examples_file(&examples.file) {
                                return format!("{}\n\n{}", template.system, examples_content);
                            }
                        }
                    }

                    return template.system.clone();
                }
            }
            
            // 模板文件存在但该类型未定义
            warn!("Template type '{}' not found in prompt.toml, using built-in", type_key);
        } else {
            // 配置未加载（文件不存在或解析失败）
            warn!("prompt.toml not loaded, using built-in template for '{}'", type_key);
        }

        // Fallback 到内置模板
        info!("→ Using built-in template for '{}' (prompt.toml not available)", type_key);
        self.build_builtin_template()
    }

    /// 获取图纸类型的键名
    fn get_type_key(&self) -> String {
        match &self.drawing_type {
            DrawingType::BuildingPlan => "building_plan".to_string(),
            DrawingType::StructurePlan => "structure_plan".to_string(),
            DrawingType::Reinforcement => "reinforcement".to_string(),
            DrawingType::RoadSection => "road_section".to_string(),
            DrawingType::Foundation => "foundation".to_string(),
            DrawingType::Custom(s) => {
                // 支持涵洞模板类型（从 internal_id 转换）
                // 检查是否是涵洞模板类型
                let culvert_types = [
                    "culvert_setting_table", "culvert_quantity_table", "culvert_layout",
                    "dark_culvert_layout", "box_culvert_reinforcement_2m", 
                    "box_culvert_reinforcement_3m", "box_culvert_reinforcement_4m",
                    "skewed_box_culvert_reinforcement_2m", "skewed_box_culvert_reinforcement_3m",
                    "skewed_box_culvert_reinforcement_4m", "joint_waterproofing",
                    "culvert_length_adjustment", "water_stop_installation",
                    "cap_stone_reinforcement", "foundation_reinforcement_plan",
                    "foundation_reinforcement_side", "culvert_length_adjustment_1",
                    "culvert_length_adjustment_2", "culvert_length_adjustment_3",
                    "skewed_reinforcement_combination",
                ];
                if culvert_types.contains(&s.as_str()) {
                    s.clone()
                } else {
                    "custom".to_string()
                }
            }
        }
    }

    /// 加载示例文件内容
    fn load_examples_file(&self, file_path: &str) -> Option<String> {
        use std::fs;
        
        // 尝试从当前工作目录加载
        let path = Path::new(file_path);
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => {
                    info!("  ✓ Loaded examples file: {}", file_path);
                    // 包装为示例格式
                    return Some(format!("# 示例\n\n{}", content.trim()));
                }
                Err(e) => {
                    warn!("  ✗ Failed to load examples file '{}': {}", file_path, e);
                }
            }
        } else {
            warn!("  ✗ Examples file not found: {}", file_path);
        }
        None
    }

    /// 构建内置模板（fallback）
    fn build_builtin_template(&self) -> String {
        let type_name = match &self.drawing_type {
            DrawingType::BuildingPlan => "建筑平面图",
            DrawingType::StructurePlan => "结构平面图",
            DrawingType::Reinforcement => "结构配筋图",
            DrawingType::RoadSection => "市政道路断面图",
            DrawingType::Foundation => "基坑支护图",
            DrawingType::Custom(s) => s.as_str(),
        };

        format!(
            r#"# 角色
你是土木施工图纸审核专家，精通 CAD 图纸识读和工程量计算。

# 任务
分析这张{}，提取所有工程数值标注，按规范计算核心数值。

# 输出格式
输出 JSON：
{{
  "基础信息": {{ "图纸名称": "string", "比例尺": "string" }},
  "识别数值清单": [{{ "标注位置": "string", "原始标注": "string", "数据类型": "尺寸 | 标高 | 角度 | 数量", "置信度": "高 | 中 | 低" }}],
  "计算结果": [{{ "计算项": "string", "计算过程": "string", "最终结果": "string" }}],
  "异常说明": "string 或'无'"
}}

# 限制
- 图片质量过低：回复"图片质量过低，无法识别"
- 非 CAD 图纸：回复"这不是土木施工图纸"
- 置信度：高=清晰，中=部分模糊，低=严重模糊需人工校核"#,
            type_name
        )
    }
}

/// 从默认路径加载提示词模板
pub fn load_prompt_template(drawing_type: DrawingType) -> PromptTemplate {
    // Use configured path
    let config = crate::config::load_config();
    let template_path = PathBuf::from(config.prompt_template_path);

    info!("Loading prompt template from: {}", template_path.display());

    if template_path.exists() {
        info!("✓ Template file exists");
        match PromptTemplate::load_from_file(&template_path) {
            Ok(template) => {
                info!("✓ Successfully loaded and parsed prompt.toml");
                return template.with_drawing_type(drawing_type);
            }
            Err(e) => {
                error!("✗ Failed to parse prompt.toml: {}", e);
                warn!("Falling back to built-in templates");
            }
        }
    } else {
        warn!("Template file not found: {}", template_path.display());
        warn!("Create this file to use custom prompts (see templates/prompt.toml.example)");
    }

    // Fallback to built-in templates
    info!("→ Using built-in prompt templates");
    PromptTemplate::with_default(drawing_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_template() {
        let template = PromptTemplate::with_default(DrawingType::BuildingPlan);
        let prompt = template.build();
        
        assert!(prompt.contains("角色"));
        assert!(prompt.contains("任务"));
        assert!(prompt.contains("输出格式"));
    }

    #[test]
    fn test_type_key() {
        let template = PromptTemplate::with_default(DrawingType::BuildingPlan);
        assert_eq!(template.get_type_key(), "building_plan");
        
        let template2 = PromptTemplate::with_default(DrawingType::StructurePlan);
        assert_eq!(template2.get_type_key(), "structure_plan");
    }
}
