# Этап 2 Матрица совместимости

Повторный прогон 17 ноября из сохранённой archive-copy: первоначальная папка Downloads больше не доступна. SHA256 всех 161 листингов совпали с отчётом 15 ноября. Воспроизведение не изменяло исходники корпуса.

Сборка baseline → итог: Auto 117/161 → 117/161; A86 117/161 → 117/161; встроенный NASM 12/161 → 12/161; MASM 18/161 → 18/161; внешний NASM 22/161 → 22/161. Изменение scasw.asm относится к отдельному примеру, не к корпусу книги.

Auto: 117 попыток запуска; наблюдаемые исходы {'terminated': 89, 'halted': 8, 'instruction_limit': 17, 'needs_input': 2, 'cpu_error': 1}. Эти значения НЕ являются run-pass или semantic-pass. Для 161 листинга полноценного семантического оракула нет; semantic-pass корпуса = не измерен.

Классификация отказов: {'assemble_pass': 117, 'execute_attempted': 117, 'terminated': 89, 'invalid/incomplete-fragment': 24, 'instruction_limit': 17, 'halted': 8, 'cpu_error': 1, 'unsupported/review': 3, 'needs_input': 2, 'out-of-scope': 17}

Новые быстрые семантические регрессии: SCASW AX/CX/ES:DI/ZF и указатель на начало слова; A86 include/макрос/0100/021 с байтами и выводом; MIPS псевдоинструкции, ввод, память и два отдельных режима delay slot. Существующие NASM и MIPS binutils comparisons сохранены. Полная ISA и соответствие оригинальному A86 побайтно не заявляются.

Исправления: scasw.asm теперь использует SUB DI,2; повторный запуск retained corpus исключает дублирование сгенерированных quiz fragments (2 Python-регрессии). Ядра CPU не изменялись.

Команды:
```sh
cargo build --offline -p rustemu-cli
/usr/bin/python3 scripts/test_book_corpus.py
/usr/bin/python3 scripts/check_book_corpus.py target/book-corpus-a86-reviewed/archive-copy --emulator target/debug/rustemu --out target/book-corpus-2026-11-17-final
/usr/bin/python3 scripts/summarize_compatibility.py target/book-corpus-2026-11-17-final/results.json --previous target/book-corpus-a86-reviewed/results.json
cargo test --offline -p rustemu-session --test compatibility_semantics
```

[Классификация отказов](a86-failures-2026-11-17.md). [JSON и длительности запусков](../target/book-corpus-2026-11-17-final/results.json). [Подробные логи](../target/book-corpus-2026-11-17-final/index.html). [Итоговые проверки всех этапов](top3-final-2026-11-17.md).

Бинарник SHA256: `e4337708ad427baffd5170e263c892386eafdc173314b8f02889c1d6d375399e`. NASM: NASM version 3.01 compiled on Oct 11 2025. Ограничение: 200000 инструкций / 5 секунд, stdin EOF, IRQ таймера выключен. Различия остановок Auto и A86 при повторной сборке не выдаются за различия семантики.

## Все 161 листинг

| example | dialect | assemble | run | output | CPU и память | reason и status |
|---|---|---|---|---|---|---|
| 02/bmp_0.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 02/test1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 02/test2.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Отрицательный DUP: arr1-tail; некорректная длина массива. |
| 02/test3.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 02/var_0a.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 02/var_0b.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 02/var_2.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa1.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa10.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: NOT start: нет размера памяти у кодовой метки; фрагмент с неоднозначным операндом. |
| 03/exa11.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa12.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Отсутствуют определения word1 и byte1; неполный фрагмент. |
| 03/exa13.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Повторные определения gamma и beta в демонстрационном фрагменте. |
| 03/exa14.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa15.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Отсутствует определение l2; неполный фрагмент. |
| 03/exa16.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa17.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa18.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa2.8 | (A86 | pass | cpu_error | '\n' | not asserted (raw --regs in log) | data-only: DB/DW/DD выполнялись как код; CPU error не доказывает дефект CPU |
| 03/exa3.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa5.8 | Auto (см. диагностику) | fail | not run |  | not run | unsupported/review: AH использован как элемент данных; неоднозначность регистра и константы A86, требует решения автора. |
| 03/exa6.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa7.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa8.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/exa9.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa12.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa13.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: SI+DI не является допустимым эффективным адресом 8086; в листинге помечено ?!. |
| 04/exa14.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: ADD память-память запрещён ISA; пример содержит ?!. |
| 04/exa15.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: ADD память-память, неверные адресные регистры и прочие намеренно ошибочные строки. |
| 04/exa2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa3.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa4.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: INC 7 и ADD 3,AX недопустимы; помечены ?!. |
| 04/exa5.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: NOT [082] без размера; далее есть явные несовпадения размеров операндов. |
| 04/exa6.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa7.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa8.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/exa9.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 05/exa1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 05/exa2.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/cnt_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/cnt_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_1b.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_3.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_4.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_5.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 06/find_6.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/pack_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/pack_1b.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/pack_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/ref_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/ref_2.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: MOV AL,0100h: константа 256 не помещается в AL; строка помечена ?!. |
| 07/ref_3.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/test1.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/test11.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/test12.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/test2.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/var_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/var_1b.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 07/var_2.8 | (A86 | pass | needs_input | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/calc_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fld_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fld_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fprem1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fsave.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fst_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fst_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/fxch.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: В исходнике оставлено многоточие ...; это шаблон, а не готовая программа. |
| 08/inner.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/log_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/log_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/math_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/math_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/miss.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/power.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/round.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/test1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/test2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 08/types.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/ea1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/ea2.08 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/ea2_1.08 | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Внешние символы другого объектного модуля; нужен объектный компоновщик. |
| 09/ea2_2.08 | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Внешние символы другого объектного модуля; нужен объектный компоновщик. |
| 09/jump.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: В base оставлена заглушка 0????; требуется исходное значение среды. |
| 09/prep.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/reboot.8 | Auto (см. диагностику) | fail | not run |  | not run | unsupported/review: Редкая абсолютная сегментная запись w 0:[di] не принимается; исходник также помечает bug. |
| 09/seg_reg.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: MOV CS,DS запрещён; строка помечена ?!. |
| 09/stack_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/stack_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/steps.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/task1.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/task2.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/vid_1.8 | (A86 | pass | terminated | '\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n**********\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/vid_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 09/vid_3.8 | (A86 | pass | terminated | '\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n***\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/bound.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/brk.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/gp.asm | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 10/gp_2.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 10/gp_3.asm | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 10/gp_4.asm | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 10/int080.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/into.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_1.8 | (A86 | pass | instruction_limit | '\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n*\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_2.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_3.8 | (A86 | pass | terminated | '\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n**\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_4.8 | (A86 | pass | terminated | '\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n**\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_5a.8 | (A86 | pass | terminated | 'Hi!\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/isr_5b.8 | (A86 | pass | terminated | 'Hi!\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/trace.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 10/tsr1k.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 11/i1c_1.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 11/i8.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 11/i9.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 11/rtc.8 | (A86 | pass | needs_input | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 11/sw_irq0.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 12/a32_str.asm | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/a4_str/a4_str.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/cmd_1/cmd_1.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/cmd_2.asm | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/cx_ecx/cx_ecx.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/find_3/find_3.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/ind32/ind32.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/mul_div/mul_div.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/new_cmd/new_cmd.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/o4_ret/o4_ret.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 12/xlat/xlat.8x | Auto (см. диагностику) | fail | not run |  | not run | out-of-scope: Требует 386/486-регистры или префиксы; не входит в 8086/80186. |
| 03/quiz1.txt.part-01.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-02.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: BIT 16 + адрес не помещается в DW; упражнение с ошибкой диапазона. |
| 03/quiz1.txt.part-03.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-04.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-05.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-06.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-07.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-08.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz1.txt.part-09.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz2.txt.part-01.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz2.txt.part-02.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz2.txt.part-03.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Отрицательная длина/значение в DB и неразрешённые границы end. |
| 03/quiz2.txt.part-04.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz2.txt.part-05.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Размер DUP зависит от своей конечной метки; сборка не сходится. |
| 03/quiz2.txt.part-06.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz2.txt.part-07.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Отсутствует определение gamma. |
| 03/quiz3.txt.part-01.8 | Auto (см. диагностику) | fail | not run |  | not run | unsupported/review: Неподдержанный разбор составного DUP/forward-выражения; требует отдельного уточнения A86. |
| 03/quiz3.txt.part-02.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Размер DUP зависит от конечной метки; сборка не сходится. |
| 03/quiz3.txt.part-03.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 03/quiz3.txt.part-04.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Значение выражения gamma-alfa выходит за диапазон DB. |
| 03/quiz3.txt.part-05.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: DUP $-msg2 даёт отрицательный размер при последующем msg2. |
| 04/quiz1.txt.part-01.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz1.txt.part-02.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz1.txt.part-03.8 | (A86 | pass | instruction_limit | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz1.txt.part-04.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz1.txt.part-05.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz1.txt.part-06.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz2.txt.part-01.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz2.txt.part-02.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Константа как назначение MOV; далее неверное число операндов. |
| 04/quiz2.txt.part-03.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Размер ADD/INC памяти не указан; неоднозначность учебного задания. |
| 04/quiz2.txt.part-04.8 | (A86 | pass | terminated | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz2.txt.part-05.8 | (A86 | pass | halted | '\n' | not asserted (raw --regs in log) | assembled; semantic-unverified |
| 04/quiz2.txt.part-06.8 | Auto (см. диагностику) | fail | not run |  | not run | invalid/incomplete-fragment: Константа как назначение ADD; далее несовпадение AL/CX. |
