//! 图纸领域模型
//!
//! 统一的图纸类型定义，支持通用类型和涵洞专用类型

use chrono::{DateTime, Utc};

/// 涵洞图纸类型枚举（18 种模板类型）
/// 
/// 这些类型是从原 `CulvertDrawingType` 迁移过来的，
/// 用于支持涵洞图纸的自动分类和分析。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CulvertType {
    // 表格类
    CulvertSettingTable,      // 涵洞设置一览表
    CulvertQuantityTable,     // 涵洞工程数量表

    // 布置图类
    CulvertLayout,            // 涵洞布置图
    DarkCulvertLayout,        // 暗涵一般布置图（分离式）

    // 钢筋构造图类（按孔径）
    BoxCulvertReinforcement2m,  // 2m 孔径箱涵涵身钢筋构造图
    BoxCulvertReinforcement3m,  // 3m 孔径箱涵涵身钢筋构造图
    BoxCulvertReinforcement4m,  // 4m 孔径箱涵涵身钢筋构造图

    // 斜涵类
    SkewedBoxCulvertReinforcement2m,  // 30°斜度 2m 孔径
    SkewedBoxCulvertReinforcement3m,  // 30°斜度 3m 孔径
    SkewedBoxCulvertReinforcement4m,  // 30°斜度 4m 孔径

    // 细部构造类
    JointWaterproofing,         // 涵身接缝防水及基础钢筋构造图
    CulvertLengthAdjustment,    // 涵长调整及帽石、基础钢筋设计图
    WaterStopInstallation,      // 止水带安装示意图
    CapStoneReinforcement,      // 帽石钢筋布置图
    FoundationReinforcementPlan,    // 基础钢筋网平面布置图
    FoundationReinforcementSide,    // 基础钢筋网侧面布置图

    // 方案图类
    CulvertLengthAdjustment1,   // 涵长调整方案图（一）
    CulvertLengthAdjustment2,   // 涵长调整方案图（二）
    CulvertLengthAdjustment3,   // 涵长调整方案图（三）

    // 斜布钢筋类
    SkewedReinforcementCombination, // 斜涵斜布钢筋组合图
}

impl CulvertType {
    /// 获取类型名称（用于显示）
    pub fn as_str(&self) -> &str {
        match self {
            Self::CulvertSettingTable => "涵洞设置一览表",
            Self::CulvertQuantityTable => "涵洞工程数量表",
            Self::CulvertLayout => "涵洞布置图",
            Self::DarkCulvertLayout => "暗涵一般布置图（分离式）",
            Self::BoxCulvertReinforcement2m => "2m 孔径箱涵涵身钢筋构造图",
            Self::BoxCulvertReinforcement3m => "3m 孔径箱涵涵身钢筋构造图",
            Self::BoxCulvertReinforcement4m => "4m 孔径箱涵涵身钢筋构造图",
            Self::SkewedBoxCulvertReinforcement2m => "30°斜度 2m 孔径箱涵钢筋构造图",
            Self::SkewedBoxCulvertReinforcement3m => "30°斜度 3m 孔径箱涵钢筋构造图",
            Self::SkewedBoxCulvertReinforcement4m => "30°斜度 4m 孔径箱涵钢筋构造图",
            Self::JointWaterproofing => "涵身接缝防水及基础钢筋构造图",
            Self::CulvertLengthAdjustment => "涵长调整及帽石、基础钢筋设计图",
            Self::WaterStopInstallation => "止水带安装示意图",
            Self::CapStoneReinforcement => "帽石钢筋布置图",
            Self::FoundationReinforcementPlan => "基础钢筋网平面布置图",
            Self::FoundationReinforcementSide => "基础钢筋网侧面布置图",
            Self::CulvertLengthAdjustment1 => "涵长调整方案图（一）",
            Self::CulvertLengthAdjustment2 => "涵长调整方案图（二）",
            Self::CulvertLengthAdjustment3 => "涵长调整方案图（三）",
            Self::SkewedReinforcementCombination => "斜涵斜布钢筋组合图",
        }
    }

    /// 获取所有预定义的涵洞类型
    pub fn get_all_types() -> &'static [CulvertType] {
        &[
            CulvertType::CulvertSettingTable,
            CulvertType::CulvertQuantityTable,
            CulvertType::CulvertLayout,
            CulvertType::DarkCulvertLayout,
            CulvertType::BoxCulvertReinforcement2m,
            CulvertType::BoxCulvertReinforcement3m,
            CulvertType::BoxCulvertReinforcement4m,
            CulvertType::SkewedBoxCulvertReinforcement2m,
            CulvertType::SkewedBoxCulvertReinforcement3m,
            CulvertType::SkewedBoxCulvertReinforcement4m,
            CulvertType::JointWaterproofing,
            CulvertType::CulvertLengthAdjustment,
            CulvertType::WaterStopInstallation,
            CulvertType::CapStoneReinforcement,
            CulvertType::FoundationReinforcementPlan,
            CulvertType::FoundationReinforcementSide,
            CulvertType::CulvertLengthAdjustment1,
            CulvertType::CulvertLengthAdjustment2,
            CulvertType::CulvertLengthAdjustment3,
            CulvertType::SkewedReinforcementCombination,
        ]
    }

    /// 转换为内部标识符（用于 API 传输）
    pub fn to_internal_id(&self) -> String {
        match self {
            Self::CulvertSettingTable => "culvert_setting_table",
            Self::CulvertQuantityTable => "culvert_quantity_table",
            Self::CulvertLayout => "culvert_layout",
            Self::DarkCulvertLayout => "dark_culvert_layout",
            Self::BoxCulvertReinforcement2m => "box_culvert_reinforcement_2m",
            Self::BoxCulvertReinforcement3m => "box_culvert_reinforcement_3m",
            Self::BoxCulvertReinforcement4m => "box_culvert_reinforcement_4m",
            Self::SkewedBoxCulvertReinforcement2m => "skewed_box_culvert_reinforcement_2m",
            Self::SkewedBoxCulvertReinforcement3m => "skewed_box_culvert_reinforcement_3m",
            Self::SkewedBoxCulvertReinforcement4m => "skewed_box_culvert_reinforcement_4m",
            Self::JointWaterproofing => "joint_waterproofing",
            Self::CulvertLengthAdjustment => "culvert_length_adjustment",
            Self::WaterStopInstallation => "water_stop_installation",
            Self::CapStoneReinforcement => "cap_stone_reinforcement",
            Self::FoundationReinforcementPlan => "foundation_reinforcement_plan",
            Self::FoundationReinforcementSide => "foundation_reinforcement_side",
            Self::CulvertLengthAdjustment1 => "culvert_length_adjustment_1",
            Self::CulvertLengthAdjustment2 => "culvert_length_adjustment_2",
            Self::CulvertLengthAdjustment3 => "culvert_length_adjustment_3",
            Self::SkewedReinforcementCombination => "skewed_reinforcement_combination",
        }.to_string()
    }

    /// 从内部标识符转换
    pub fn from_internal_id(id: &str) -> Option<Self> {
        match id {
            "culvert_setting_table" => Some(Self::CulvertSettingTable),
            "culvert_quantity_table" => Some(Self::CulvertQuantityTable),
            "culvert_layout" => Some(Self::CulvertLayout),
            "dark_culvert_layout" => Some(Self::DarkCulvertLayout),
            "box_culvert_reinforcement_2m" => Some(Self::BoxCulvertReinforcement2m),
            "box_culvert_reinforcement_3m" => Some(Self::BoxCulvertReinforcement3m),
            "box_culvert_reinforcement_4m" => Some(Self::BoxCulvertReinforcement4m),
            "skewed_box_culvert_reinforcement_2m" => Some(Self::SkewedBoxCulvertReinforcement2m),
            "skewed_box_culvert_reinforcement_3m" => Some(Self::SkewedBoxCulvertReinforcement3m),
            "skewed_box_culvert_reinforcement_4m" => Some(Self::SkewedBoxCulvertReinforcement4m),
            "joint_waterproofing" => Some(Self::JointWaterproofing),
            "culvert_length_adjustment" => Some(Self::CulvertLengthAdjustment),
            "water_stop_installation" => Some(Self::WaterStopInstallation),
            "cap_stone_reinforcement" => Some(Self::CapStoneReinforcement),
            "foundation_reinforcement_plan" => Some(Self::FoundationReinforcementPlan),
            "foundation_reinforcement_side" => Some(Self::FoundationReinforcementSide),
            "culvert_length_adjustment_1" => Some(Self::CulvertLengthAdjustment1),
            "culvert_length_adjustment_2" => Some(Self::CulvertLengthAdjustment2),
            "culvert_length_adjustment_3" => Some(Self::CulvertLengthAdjustment3),
            "skewed_reinforcement_combination" => Some(Self::SkewedReinforcementCombination),
            _ => None,
        }
    }
}

/// 图纸类型 - 统一枚举
/// 
/// 支持：
/// 1. 通用图纸类型（装配图、零件图、原理图等）
/// 2. 涵洞专用类型（18 种涵洞图纸类型）
/// 3. 自定义类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawingType {
    // 通用类型
    Assembly,      // 装配图
    Part,          // 零件图
    Schematic,     // 原理图
    Piping,        // 管道图
    Electrical,    // 电气图
    
    // 涵洞专用类型
    Culvert(CulvertType),
    
    // 自定义类型
    Custom(String),
}

impl DrawingType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Assembly => "装配图",
            Self::Part => "零件图",
            Self::Schematic => "原理图",
            Self::Piping => "管道图",
            Self::Electrical => "电气图",
            Self::Culvert(culvert_type) => culvert_type.as_str(),
            Self::Custom(s) => s.as_str(),
        }
    }

    /// 判断是否为涵洞类型
    pub fn is_culvert_type(&self) -> bool {
        matches!(self, DrawingType::Culvert(_))
    }

    /// 获取涵洞类型（如果是）
    pub fn as_culvert_type(&self) -> Option<&CulvertType> {
        match self {
            DrawingType::Culvert(t) => Some(t),
            _ => None,
        }
    }
}

impl std::str::FromStr for DrawingType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 首先尝试匹配涵洞类型
        if let Some(culvert_type) = CulvertType::from_internal_id(s) {
            return Ok(DrawingType::Culvert(culvert_type));
        }

        // 然后尝试匹配通用类型
        match s.to_lowercase().as_str() {
            "assembly" | "装配图" => Ok(Self::Assembly),
            "part" | "零件图" => Ok(Self::Part),
            "schematic" | "原理图" => Ok(Self::Schematic),
            "piping" | "管道图" => Ok(Self::Piping),
            "electrical" | "电气图" => Ok(Self::Electrical),
            _ => Ok(Self::Custom(s.to_string())),
        }
    }
}

impl std::fmt::Display for DrawingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 图纸实体
#[derive(Debug, Clone)]
pub struct Drawing {
    pub id: String,
    pub drawing_type: DrawingType,
    pub image_data: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

impl Drawing {
    pub fn new(
        drawing_type: DrawingType,
        image_data: Vec<u8>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            drawing_type,
            image_data,
            created_at: Utc::now(),
        }
    }
    
    /// 验证图片数据
    pub fn validate_image(&self) -> Result<(), crate::domain::DomainError> {
        // 基本验证：非空
        if self.image_data.is_empty() {
            return Err(crate::domain::DomainError::validation("image_data", "Image data cannot be empty"));
        }

        // 验证文件大小（最大 10MB）
        const MAX_SIZE: usize = 10 * 1024 * 1024;
        if self.image_data.len() > MAX_SIZE {
            return Err(crate::domain::DomainError::validation("image_data", format!("Image size exceeds maximum ({} > {})", self.image_data.len(), MAX_SIZE)));
        }

        Ok(())
    }
}

/// 图纸分析结果
#[derive(Debug, Clone)]
pub struct DrawingAnalysis {
    pub drawing_id: String,
    pub content: String,
    pub model_used: String,
    pub latency_ms: u64,
    pub analyzed_at: DateTime<Utc>,
}

impl DrawingAnalysis {
    pub fn new(
        drawing_id: impl Into<String>,
        content: impl Into<String>,
        model_used: impl Into<String>,
        latency_ms: u64,
    ) -> Self {
        Self {
            drawing_id: drawing_id.into(),
            content: content.into(),
            model_used: model_used.into(),
            latency_ms,
            analyzed_at: Utc::now(),
        }
    }
}
