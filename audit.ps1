#нужно установить cargo install cargo-audit cargo-deny cargo-make
$reportDir = ".\reports"
if (!(Test-Path $reportDir)) { New-Item -ItemType Directory -Path $reportDir }

$timestamp = Get-Date -Format "yyyyMMdd_HHmm"
$logFile = "$reportDir\audit_$timestamp.log"

Function Log-Step {
    param([string]$Message)
    Write-Host ">>> $Message" -ForegroundColor Cyan
    Add-Content -Path $logFile -Value ">>> $Message"
}

Log-Step "Запуск полного аудита проекта..."

Log-Step "Проверка стиля (rustfmt)..."
cargo fmt -- --check 2>&1 | Out-File -Append $logFile

Log-Step "Статический анализ (Clippy)..."
cargo clippy --message-format=json 2>&1 | Out-File -Append $logFile

Log-Step "Аудит зависимостей (cargo-audit)..."
cargo audit 2>&1 | Out-File -Append $logFile

Log-Step "Проверка политик (cargo-deny)..."
cargo deny check 2>&1 | Out-File -Append $logFile

Write-Host "--------------------------------" -ForegroundColor Green
Write-Host "Аудит завершен. Лог сохранен в: $logFile" -ForegroundColor Green