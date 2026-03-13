#!/bin/bash
# 清除 .env 文件的 git 历史并检查 API Key 泄露
#
# 使用方法：
#   ./scripts/clean_env.sh
#
# 安全说明：
#   1. 此脚本会清除 .env 文件的所有 git 历史记录
#   2. 检查 API Key 是否已经泄露到 git 历史
#   3. 如果已泄露，必须立即轮换 API Key
#   4. 检查当前 .env 文件是否包含真实 API Key（可能意外提交）

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "🔒 开始检查 API Key 安全性并清除 .env 历史..."
echo ""

# ===== 第零步：检查当前 .env 文件 =====
echo "📋 第零步：检查当前 .env 文件..."

if [ -f ".env" ]; then
    echo -e "${YELLOW}⚠️  发现 .env 文件${NC}"
    
    # 检查是否包含 API Key 模式的字符串
    if grep -q "OLLAMA_API_KEY=sk-" .env 2>/dev/null; then
        echo -e "${RED}⚠️  警告：.env 文件包含真实 API Key（以 sk- 开头）${NC}"
        echo ""
        echo "建议："
        echo "  1. 确保 .env 已添加到 .gitignore"
        echo "  2. 不要将 .env 提交到 git"
        echo "  3. 考虑使用环境变量或密钥管理服务"
        echo ""
        
        # 检查 .gitignore
        if grep -q "^\.env$" .gitignore 2>/dev/null; then
            echo -e "${GREEN}✓ .env 已在 .gitignore 中${NC}"
        else
            echo -e "${RED}✗ .env 未在 .gitignore 中！${NC}"
            echo ""
            read -p "是否自动添加到 .gitignore? (y/n): " add_to_ignore
            if [ "$add_to_ignore" = "y" ]; then
                echo ".env" >> .gitignore
                echo -e "${GREEN}✓ 已添加到 .gitignore${NC}"
            fi
        fi
    else
        echo -e "${GREEN}✓ .env 文件未包含明显 API Key${NC}"
    fi
else
    echo -e "${GREEN}✓ 未发现 .env 文件（使用环境变量或 .env.example）${NC}"
fi

echo ""

# ===== 第一步：检查 API Key 是否已泄露 =====
echo "📋 第一步：检查 API Key 是否已泄露到 git 历史..."

# 检查 .env 文件是否在 git 历史中
if git log --all --full-history --pretty=format:"%h %s" -- .env | head -1 > /dev/null 2>&1; then
    echo -e "${RED}⚠️  警告：.env 文件存在于 git 历史中！${NC}"
    echo ""
    echo "提交记录："
    git log --all --full-history --pretty=format:"  %h - %s (%ar)" -- .env | head -10
    echo ""

    # 检查是否有 API Key 模式的字符串
    echo "🔍 检查是否有 API Key 模式的字符串..."
    if git log --all -p | grep -i "OLLAMA_API_KEY=" | head -5 > /dev/null 2>&1; then
        echo -e "${RED}⚠️  严重：发现 API Key 已提交到 git 历史！${NC}"
        echo ""
        echo "泄露的 Key 片段："
        git log --all -p | grep -i "OLLAMA_API_KEY=" | head -5
        echo ""
        echo -e "${RED}🚨 必须立即执行以下操作：${NC}"
        echo ""
        echo -e "${BLUE}【紧急行动清单】${NC}"
        echo "  1️⃣  立即轮换 API Key："
        echo "     前往 Ollama 官网 (https://ollama.com/connect) 生成新的 API Key"
        echo ""
        echo "  2️⃣  检查 API Key 使用日志："
        echo "     确认有无可疑调用（异常时间、异常地点、异常频率）"
        echo ""
        echo "  3️⃣  启用 API Key 过期时间（如果支持）："
        echo "     设置 API Key 自动过期，降低长期泄露风险"
        echo ""
        echo "  4️⃣  考虑启用 IP 白名单（如果支持）："
        echo "     限制 API Key 只能从特定 IP 地址使用"
        echo ""
        echo "  5️⃣  通知团队成员："
        echo "     如果有其他人可能使用此 API Key，通知他们已泄露"
        echo ""
        read -p "确认已轮换 API Key (y/n): " confirm
        if [ "$confirm" != "y" ]; then
            echo -e "${YELLOW}⚠️  警告：API Key 未轮换，存在安全风险！${NC}"
            echo ""
            echo "请在执行完上述操作后，再次运行此脚本。"
            exit 1
        else
            echo -e "${GREEN}✓ 确认已轮换 API Key${NC}"
        fi
    fi
else
    echo -e "${GREEN}✓ .env 文件未出现在 git 历史中${NC}"
fi

echo ""

# ===== 第二步：检查远程仓库 =====
echo "📋 第二步：检查远程仓库配置..."
if git remote -v | head -1 > /dev/null 2>&1; then
    echo "远程仓库："
    git remote -v
    echo ""
    echo -e "${YELLOW}⚠️  注意：如果 .env 已推送到远程仓库，本地清理无效！${NC}"
    echo "   必须执行 git push --force 覆盖远程历史"
    echo ""
    read -p "是否自动覆盖远程历史？(y/n): " push_force
    if [ "$push_force" = "y" ]; then
        echo "覆盖远程历史..."
        git push --force --all
        git push --force --tags
        echo -e "${GREEN}✓ 已覆盖远程历史${NC}"
    else
        echo -e "${YELLOW}⚠️  请手动执行：git push --force --all${NC}"
    fi
else
    echo -e "${GREEN}✓ 无远程仓库（本地仓库）${NC}"
fi

echo ""

# ===== 第三步：清除 .env 历史 =====
echo "📋 第三步：清除 .env 文件的 git 历史..."

# 方法 1: 使用 git filter-branch
echo "使用 git filter-branch..."
git filter-branch --force --index-filter \
  "git rm --cached --ignore-unmatch .env" \
  --prune-empty --tag-name-filter cat -- --all

# 清理引用
rm -rf .git/refs/original/
git reflog expire --expire=now --all
git gc --prune=now --aggressive

echo ""
echo -e "${GREEN}✅ 清除完成！${NC}"
echo ""

# ===== 第四步：验证清理结果 =====
echo "📋 第四步：验证清理结果..."
echo ""
echo "验证命令：git log --all --full-history -- .env"
if git log --all --full-history -- .env | head -1 > /dev/null 2>&1; then
    echo -e "${YELLOW}⚠️  警告：.env 历史可能未完全清除${NC}"
else
    echo -e "${GREEN}✓ .env 历史已清除${NC}"
fi

echo ""
echo "===== 下一步操作 ====="
echo ""
echo "1. 如果有远程仓库，执行以下命令覆盖远程历史："
echo -e "   ${YELLOW}git push --force --all${NC}"
echo "   ${YELLOW}git push --force --tags${NC}"
echo ""
echo "2. 通知团队成员执行以下命令同步仓库："
echo -e "   ${YELLOW}git fetch origin${NC}"
echo -e "   ${YELLOW}git reset --hard @{u}${NC}"
echo ""
echo "3. 复制 .env.example 为 .env 并填入新的 API Key："
echo "   cp .env.example .env"
echo ""
echo "4. 将 .env 添加到 .gitignore（如果还没有）："
echo "   echo '.env' >> .gitignore"
echo ""
echo "5. 考虑启用 API Key 过期时间或 IP 白名单"
echo ""
echo "6. 定期检查 git 历史，确保敏感信息未泄露："
echo "   git log --all -p | grep -i 'API_KEY'"
echo ""
echo -e "${GREEN}🔐 安全提示：使用密钥管理服务（如 AWS Secrets Manager、HashiCorp Vault）管理敏感信息${NC}"
