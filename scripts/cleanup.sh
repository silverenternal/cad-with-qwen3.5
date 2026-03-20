#!/bin/bash

# CAD 项目清理脚本
# 用于清理本地生成的临时文件、编译产物等

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║                    CAD 项目清理脚本                       ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

cd "$PROJECT_ROOT"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 清理函数
cleanup_dir() {
    local dir=$1
    local description=$2
    
    if [ -d "$dir" ]; then
        echo -e "${YELLOW}清理：$description ($dir)${NC}"
        rm -rf "$dir"
        echo -e "${GREEN}✓ 已清理：$dir${NC}"
    fi
}

cleanup_files() {
    local pattern=$1
    local description=$2
    
    local count=$(find . -maxdepth 1 -name "$pattern" 2>/dev/null | wc -l | tr -d ' ')
    if [ "$count" -gt 0 ]; then
        echo -e "${YELLOW}清理：$description ($count 个文件)${NC}"
        find . -maxdepth 1 -name "$pattern" -exec rm -f {} \;
        echo -e "${GREEN}✓ 已清理：$pattern${NC}"
    fi
}

echo "开始清理..."
echo ""

# 清理编译产物
cleanup_dir "target" "Rust 编译产物"
cleanup_dir "frontend/target" "Frontend 编译产物"

# 清理依赖目录
cleanup_dir "frontend/node_modules" "Frontend 依赖"

# 清理运行时生成的文件
cleanup_dir "data" "数据库文件"
cleanup_dir "logs" "日志文件"
cleanup_dir "telemetry" "遥测数据"
cleanup_dir "tmp" "临时文件"
cleanup_dir "temp" "临时文件"

# 清理测试数据
cleanup_dir "test_batch" "测试批量数据"
cleanup_dir "cad_image" "测试图片"

# 清理报告文件
cleanup_files "pdf_report_*.md" "PDF 报告"
cleanup_files "batch_results_*.json" "批量结果 JSON"
cleanup_files "batch_results_*.csv" "批量结果 CSV"
cleanup_files "dialog_*.md" "对话历史"
cleanup_files "export_*" "导出目录"
cleanup_files "*.tar.gz" "归档文件"
cleanup_files "cad_data_*.json" "CAD 数据"
cleanup_files "clippy_*.txt" "Clippy 输出"
cleanup_files "cargo_check.log" "Cargo 检查日志"

# 清理前端报告文件
cleanup_dir "frontend/dist" "Frontend 构建产物"
cleanup_files "frontend/*_REPORT*.md" "前端报告"

# 清理结构验证目录
cleanup_dir "structure_ensure" "结构验证目录"

# 清理模板示例
cleanup_dir "templates/examples" "模板示例"

echo ""
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    清理完成！                             ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# 显示磁盘空间节省
echo "当前项目大小:"
du -sh "$PROJECT_ROOT" 2>/dev/null | cut -f1

echo ""
echo "提示：运行 'git status' 查看清理后的状态"
