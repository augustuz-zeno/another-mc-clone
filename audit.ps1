# Убедись, что инструменты установлены: cargo install cargo-audit cargo-deny cargo-make

$reportDir = ".\reports"
if (!(Test-Path $reportDir)) { New-Item -ItemType Directory -Path $reportDir }

$timestamp = Get-Date -Format "yyyyMMdd_HHmm"
$logFile = "$reportDir\audit_$timestamp.log"
$summaryFile = "full_log.txt"

# Очистка старого итогового файла
if (Test-Path $summaryFile) { Remove-Item $summaryFile }

Function Log-Step {
    param([string]$Message)
    Write-Host ">>> $Message" -ForegroundColor Cyan
    Add-Content -Path $logFile -Value "`n>>> $Message"
}

Log-Step "Запуск полного аудита проекта..."

Log-Step "Проверка стиля (rustfmt)..."
cargo fmt -- --check 2>&1 | Out-File -Append $logFile

Log-Step "Статический анализ (Clippy)..."
# Убираем --message-format=json для читаемости людьми и нейросетями
cargo clippy 2>&1 | Out-File -Append $logFile

Log-Step "Аудит зависимостей (cargo-audit)..."
cargo audit 2>&1 | Out-File -Append $logFile

Log-Step "Проверка политик (cargo-deny)..."
cargo deny check 2>&1 | Out-File -Append $logFile

# Копируем текущий лог в итоговый файл
Copy-Item -Path $logFile -Destination $summaryFile

Write-Host "--------------------------------" -ForegroundColor Green
Write-Host "Аудит завершен." -ForegroundColor Green
Write-Host "Лог: $logFile" -ForegroundColor Yellow
Write-Host "Итоговый файл для ИИ: $summaryFile" -ForegroundColor Yellow