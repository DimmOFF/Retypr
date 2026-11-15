# Релизы и GitHub Actions

## Что настроено
| Файл | Когда запускается | Что делает |
|---|---|---|
| `.github/workflows/ci.yml` | каждый push в `main` и каждый pull request | `cargo test` и release-сборка на Windows (обязательно), macOS и Linux (экспериментально — их падение не краснит проверку) |
| `.github/workflows/release.yml` | push тега `v*` или вручную (Actions → Release → Run workflow) | собирает `Retypr-<версия>-windows-x64.zip`, `Retypr-<версия>-macos-universal.dmg`, `Retypr-<версия>-linux-x86_64.tar.gz`, считает SHA256 и создаёт **черновик** GitHub Release |

## Первый запуск
```bash
cd retypr                      # корень проекта: здесь лежат Cargo.toml и папка .github
git init
git add .
git status                     # убедитесь, что в списке есть Cargo.lock и .github/workflows
git commit -m "Initial commit"
git branch -M main
git remote add origin https://github.com/<ваш-логин>/retypr.git
git push -u origin main
```
Затем откройте вкладку **Actions** репозитория: там появится запуск CI.

## Выпуск версии
1. Поднимите `version` в `Cargo.toml` (например `0.1.0`) и закоммитьте.
2. Поставьте тег и отправьте его:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
   Тег **должен совпадать** с версией в `Cargo.toml` (`v0.1.0` ↔ `0.1.0`), иначе сборка остановится с понятной ошибкой.
3. Когда сборка завершится, откройте **Releases** → черновик → проверьте файлы → **Publish release**.

Без тега: Actions → Release → Run workflow. Готовые пакеты появятся внизу страницы запуска в разделе **Artifacts**.

## Подпись macOS (необязательно, нужна платная учётная запись Apple Developer)
Settings → Secrets and variables → Actions → New repository secret:
| Секрет | Что это |
|---|---|
| `APPLE_CERTIFICATE` | сертификат «Developer ID Application» в формате .p12, закодированный в base64: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | пароль от .p12 |
| `APPLE_SIGNING_IDENTITY` | например `Developer ID Application: Имя (TEAMID)` |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | для нотаризации; пароль — «app-specific password» с appleid.apple.com |
Без секретов DMG подписывается ad-hoc (на чужом Mac открывается через правую кнопку → Open).

## Заметки
* Windows: exe без цифровой подписи, SmartScreen может показать предупреждение «Неизвестный издатель».
* Минуты Actions: для публичных репозиториев бесплатно; для приватных — квота, macOS «дороже» Windows и Linux.
* Окончания строк закреплены в `.gitattributes`: `.sh` — LF, `.cmd` — CRLF (иначе скрипты ломаются).
