# VFiles 开发启动脚本

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  VFiles - 文件管理系统" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 检查Bun是否安装
if (-not (Get-Command bun -ErrorAction SilentlyContinue)) {
    Write-Host "错误: 未找到 Bun" -ForegroundColor Red
    Write-Host "请访问 https://bun.sh 安装 Bun" -ForegroundColor Yellow
    exit 1
}

Write-Host "正在启动服务器..." -ForegroundColor Green
Write-Host ""

# 启动后端服务器
Write-Host "启动后端服务器 (端口 3000)..." -ForegroundColor Yellow
Start-Process pwsh -ArgumentList "-NoExit", "-Command", "cd '$PSScriptRoot'; bun run dev"

# 等待2秒
Start-Sleep -Seconds 2

# 启动前端开发服务器
Write-Host "启动前端开发服务器 (端口 5173)..." -ForegroundColor Yellow
Start-Process pwsh -ArgumentList "-NoExit", "-Command", "cd '$PSScriptRoot'; bun run dev:client"

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  服务器启动成功！" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "后端服务器: http://localhost:3000" -ForegroundColor Cyan
Write-Host "前端应用:   http://localhost:5173" -ForegroundColor Cyan
Write-Host ""
Write-Host "按 Ctrl+C 停止服务器" -ForegroundColor Yellow
Write-Host ""
