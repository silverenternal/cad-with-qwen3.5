@echo off
REM 清除 .env 文件的 git 历史 (Windows 版本)

echo.
echo 🔒 开始清除 .env 文件的 git 历史...
echo.

REM 方法 1: 使用 git filter-branch
echo 方法 1: 使用 git filter-branch...
git filter-branch --force --index-filter ^
  "git rm --cached --ignore-unmatch .env" ^
  --prune-empty --tag-name-filter cat -- --all

REM 清理引用
if exist .git\refs\original\ rmdir /s /q .git\refs\original\
git reflog expire --expire=now --all
git gc --prune=now --aggressive

echo.
echo ✅ 清除完成！
echo.
echo ⚠️  下一步操作：
echo 1. 去 Ollama 官网 (https://ollama.com/connect) 轮换 API Key
echo 2. 复制 .env.example 为 .env 并填入新 Key
echo 3. 如果有远程仓库，执行：git push --force --all
echo.
echo 📋 验证命令：git log --all --full-history -- .env
echo    (应该无输出)
echo.

pause
