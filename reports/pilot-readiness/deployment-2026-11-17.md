# Раскатка PILOT-READINESS V1 — 17 ноября 2026

По отдельной команде пользователя кандидат опубликован на существующем GitHub Pages:
https://olegdurandin.github.io/RustEmu8086/?ui=tablet&v=c89ee88

Commit: `c89ee88fb1e19466f4b6794d7012e698934adbbe`, предыдущий: `181b02bc220f88dba8b88f3ff3b09105c47f19db`.
[Workflow 44144001978](https://github.com/OlegDurandin/RustEmu8086/actions/runs/44144001978).
[Публичный чеклист iPad/Safari с эталонами](https://github.com/OlegDurandin/RustEmu8086/blob/main/docs/ipad-pilot-checklist.md).

## Что подтверждено

- Push в main выполнен обычным fast-forward, без force. Штатный pipeline test → web → deploy-web сохранён.
- Удалённый job `test` завершился success: Format, Clippy native/WASM, rustdoc, полный cargo test и Python corpus regressions. [Наблюдаемый ответ API](evidence/deploy-ci-jobs-observed.json). Число 499 относится к предыдущему локальному завершённому прогону; точный счёт удалённого прогона без скачанных логов не заявляется.
- Локальные сборка Pages с public-url `/RustEmu8086/`, fmt, rustdoc с -D warnings и две Python-регрессии прошли. Повторный локальный cargo test задерживался в `_dyld_start` до начала отдельных бинарников; после удалённого success остановлен только собственный process group, exit 130. Этот повтор не считается завершённым.
- [build-info.json](https://olegdurandin.github.io/RustEmu8086/build-info.json) вернул HTTP 200, новый commit и run_id 44144001978; index.html также получен по HTTP 200. Маркер записан CI после сборки и опубликован в Pages-артефакте. [Сохранённый маркер](evidence/deploy-build-info.json).
- Четыре JS/MJS-файла и WASM assembler Worker скачаны полностью; Worker имеет корректный WASM-заголовок и размер 652578 байт. Основной WASM отвечает HTTP 200/application/wasm; его URL соответствует опубликованному index.html. В viewport отсутствует запрет zoom.

## Ограничения проверки раскатки

Публичный GitHub API после успешных тестов исчерпал unauthenticated rate limit (60/hour). Опрос прекращён, другой IP/токен/драйвер для обхода не использовался. Последний наблюдаемый web job был in_progress; окончательный Actions conclusion не получен. Сам факт публикации нового commit подтверждён непосредственно сайтом, а не выведен из статуса очереди. Последний наблюдаемый macOS bundle job был in_progress; его готовность не заявляется.

Полная передача основного WASM из этой среды не уложилась в сетевой тайм-аут: без сжатия 20 секунд, с обычным gzip 90 секунд (745628 из 3392573 сжатых байт). Размер полного декодированного файла по серверу 9146057 байт. Подтверждены заголовки, MIME и полученный WASM-префикс; полная целостность/браузерное исполнение основного WASM не объявляются проверенными. Полный запуск на iPad и время первой загрузки — обязательный первый пункт ручной проверки. Тайм-аут клиента сам по себе не доказывает дефект приложения.

Физический iPad, Safari touch/IME/Files, реальный Worker/E2E и измерения Stop остаются NOT RUN. Публикация сделана для проверки кандидата по команде пользователя; готовность к внешнему пилоту по-прежнему **MANUAL VALIDATION REQUIRED**. Пользователи не приглашались.

В push включены только изменения приложения, 12 упражнений, эталоны, регрессии, CI и чеклист. Пользовательские PROJECT_AUDIT.md, SPEACH.md, TODO.md, presentations и локальные аудиты/архивы не отправлялись.

Откат: заново собрать и раскатать полный web-артефакт предыдущего commit через штатный Pages-процесс. Не делать force-push и не очищать Safari storage. Откат не запускался.

[Машиночитаемые результаты и SHA скачанных файлов](evidence/deployment-results.json).
