#!/usr/bin/env python3
"""Publish fixed-corpus assembly/execution observations without inventing semantic oracles."""
import argparse
import json
import re
from collections import Counter
from pathlib import Path

# Reviewed against the retained source text; diagnostics are also retained per row.
FRAGMENTS = {
    '02/test2.8': 'Отрицательный DUP: arr1-tail; некорректная длина массива.',
    '03/exa10.8': 'NOT start: нет размера памяти у кодовой метки; фрагмент с неоднозначным операндом.',
    '03/exa12.8': 'Отсутствуют определения word1 и byte1; неполный фрагмент.',
    '03/exa13.8': 'Повторные определения gamma и beta в демонстрационном фрагменте.',
    '03/exa15.8': 'Отсутствует определение l2; неполный фрагмент.',
    '03/exa5.8': 'AH использован как элемент данных; неоднозначность регистра и константы A86, требует решения автора.',
    '04/exa13.8': 'SI+DI не является допустимым эффективным адресом 8086; в листинге помечено ?!.',
    '04/exa14.8': 'ADD память-память запрещён ISA; пример содержит ?!.',
    '04/exa15.8': 'ADD память-память, неверные адресные регистры и прочие намеренно ошибочные строки.',
    '04/exa4.8': 'INC 7 и ADD 3,AX недопустимы; помечены ?!.',
    '04/exa5.8': 'NOT [082] без размера; далее есть явные несовпадения размеров операндов.',
    '07/ref_2.8': 'MOV AL,0100h: константа 256 не помещается в AL; строка помечена ?!.',
    '08/fxch.8': 'В исходнике оставлено многоточие ...; это шаблон, а не готовая программа.',
    '09/jump.8': 'В base оставлена заглушка 0????; требуется исходное значение среды.',
    '09/reboot.8': 'Редкая абсолютная сегментная запись w 0:[di] не принимается; исходник также помечает bug.',
    '09/seg_reg.8': 'MOV CS,DS запрещён; строка помечена ?!.',
    '12/cmd_2.asm': 'Есть BSWAP EAX: модель 486, за пределами 8086/80186.',
    '03/quiz1.txt.part-02.8': 'BIT 16 + адрес не помещается в DW; упражнение с ошибкой диапазона.',
    '03/quiz2.txt.part-03.8': 'Отрицательная длина/значение в DB и неразрешённые границы end.',
    '03/quiz2.txt.part-05.8': 'Размер DUP зависит от своей конечной метки; сборка не сходится.',
    '03/quiz2.txt.part-07.8': 'Отсутствует определение gamma.',
    '03/quiz3.txt.part-01.8': 'Неподдержанный разбор составного DUP/forward-выражения; требует отдельного уточнения A86.',
    '03/quiz3.txt.part-02.8': 'Размер DUP зависит от конечной метки; сборка не сходится.',
    '03/quiz3.txt.part-04.8': 'Значение выражения gamma-alfa выходит за диапазон DB.',
    '03/quiz3.txt.part-05.8': 'DUP $-msg2 даёт отрицательный размер при последующем msg2.',
    '04/quiz2.txt.part-02.8': 'Константа как назначение MOV; далее неверное число операндов.',
    '04/quiz2.txt.part-03.8': 'Размер ADD/INC памяти не указан; неоднозначность учебного задания.',
    '04/quiz2.txt.part-06.8': 'Константа как назначение ADD; далее несовпадение AL/CX.',
}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('results', type=Path)
    p.add_argument('--previous', type=Path, required=True)
    args = p.parse_args()
    data = json.loads(args.results.read_text())
    old = json.loads(args.previous.read_text())
    rows = [r for r in data['results'] if r['role'] == 'listing']
    previous = {r['file']: r for r in old['results'] if r['role'] == 'listing'}
    assert len(rows) == len(previous) == 161
    assert all(r['sha256'] == previous[r['file']]['sha256'] for r in rows), 'corpus changed'
    failures = []
    counts = Counter()
    def cell(s):
        return str(s).replace('|', '\\|').replace('\n', ' ')[:240]
    table = ['| example | dialect | assemble | run | output | CPU и память | reason и status |',
             '|---|---|---|---|---|---|---|']
    for r in rows:
        a = r['assembly']['auto']
        run = r.get('runs', {}).get('auto')
        syntax = re.search(r'(\S+(?:/\S+)?) syntax', a.get('stdout', ''))
        dialect = syntax[1] if syntax else 'Auto (см. диагностику)'
        if a['code'] == 0:
            counts['assemble_pass'] += 1
            counts['execute_attempted'] += 1
            counts[run['status']] += 1
            reason = 'assembled; semantic-unverified'
            if r['file'] == '03/exa2.8':
                reason = 'data-only: DB/DW/DD выполнялись как код; CPU error не доказывает дефект CPU'
        else:
            text = '\n'.join(l.split(';')[0] for l in (args.results.parent/'archive-copy'/r['file']).read_text().splitlines())
            if re.search(r'\b(?:e[abcd]x|e[sd]i|e[bs]p|o4|a32|bswap)\b', text, re.I):
                kind, reason = 'out-of-scope', 'Требует 386/486-регистры или префиксы; не входит в 8086/80186.'
            elif r['file'] in ('09/ea2_1.08', '09/ea2_2.08'):
                kind, reason = 'out-of-scope', 'Внешние символы другого объектного модуля; нужен объектный компоновщик.'
            else:
                reason = FRAGMENTS.get(r['file'])
                assert reason, 'unclassified: ' + r['file']
                kind = 'unsupported/review' if r['file'] in ('03/exa5.8', '03/quiz3.txt.part-01.8', '09/reboot.8') else 'invalid/incomplete-fragment'
            counts[kind] += 1
            reason = kind + ': ' + reason
            failures.append('| ' + ' | '.join(cell(x) for x in (r['file'], kind, reason, a['stderr'])) + ' |')
        table.append('| ' + ' | '.join(cell(x) for x in (r['file'], dialect, 'pass' if a['code']==0 else 'fail',
                     run['status'] if run else 'not run', repr(run['stdout'][:100]) if run else '',
                     'not asserted (raw --regs in log)' if run else 'not run', reason)) + ' |')
    out = Path('reports')
    (out/'a86-failures-2026-11-17.md').write_text('# Отказы корпуса A86\n\n'
        'Все 44 отказа Auto классифицированы по исходнику и диагностике. Категория unsupported/review не означает доказанный дефект: нужны оригинальный A86 и решение о поддержке редкой конструкции.\n\n'
        '| example | category | причина | первая диагностика |\n|---|---|---|---|\n'+'\n'.join(failures)+'\n')
    dialect_names = [('auto', 'Auto'), ('a86', 'A86'), ('nasm', 'встроенный NASM'), ('masm', 'MASM'), ('reference_nasm', 'внешний NASM')]
    comparison = []
    for key, label in dialect_names:
        before = sum(r['assembly'].get(key, {}).get('code') == 0 for r in previous.values())
        after = sum(r['assembly'].get(key, {}).get('code') == 0 for r in rows)
        comparison.append(f'{label} {before}/161 → {after}/161')
    runtime = {k: counts[k] for k in ('terminated', 'halted', 'instruction_limit', 'needs_input', 'cpu_error', 'timeout', 'load_error') if counts[k]}
    summary = ['# Этап 2 Матрица совместимости', '',
        'Повторный прогон 17 ноября из сохранённой archive-copy: первоначальная папка Downloads больше не доступна. SHA256 всех 161 листингов совпали с отчётом 15 ноября. Воспроизведение не изменяло исходники корпуса.', '',
        'Сборка baseline → итог: '+ '; '.join(comparison)+'. Изменение scasw.asm относится к отдельному примеру, не к корпусу книги.', '',
        f'Auto: {counts["execute_attempted"]} попыток запуска; наблюдаемые исходы {runtime}. Эти значения НЕ являются run-pass или semantic-pass. Для 161 листинга полноценного семантического оракула нет; semantic-pass корпуса = не измерен.', '',
        'Классификация отказов: '+repr(dict(counts)), '',
        'Новые быстрые семантические регрессии: SCASW AX/CX/ES:DI/ZF и указатель на начало слова; A86 include/макрос/0100/021 с байтами и выводом; MIPS псевдоинструкции, ввод, память и два отдельных режима delay slot. Существующие NASM и MIPS binutils comparisons сохранены. Полная ISA и соответствие оригинальному A86 побайтно не заявляются.', '',
        'Исправления: scasw.asm теперь использует SUB DI,2; повторный запуск retained corpus исключает дублирование сгенерированных quiz fragments (2 Python-регрессии). Ядра CPU не изменялись.', '',
        'Команды:', '```sh', 'cargo build --offline -p rustemu-cli',
        '/usr/bin/python3 scripts/test_book_corpus.py',
        f'/usr/bin/python3 scripts/check_book_corpus.py target/book-corpus-a86-reviewed/archive-copy --emulator target/debug/rustemu --out {args.results.parent}',
        f'/usr/bin/python3 scripts/summarize_compatibility.py {args.results} --previous {args.previous}',
        'cargo test --offline -p rustemu-session --test compatibility_semantics', '```', '',
        f'[Классификация отказов](a86-failures-2026-11-17.md). [JSON и длительности запусков](../{args.results}). [Подробные логи](../{args.results.parent}/index.html). [Итоговые проверки всех этапов](top3-final-2026-11-17.md).', '',
        'Бинарник SHA256: `'+data['emulator_sha256']+'`. NASM: '+str(data['nasm_version'])+'. Ограничение: 200000 инструкций / 5 секунд, stdin EOF, IRQ таймера выключен. Различия остановок Auto и A86 при повторной сборке не выдаются за различия семантики.', '',
        '## Все 161 листинг', '']
    (out/'compatibility-matrix-2026-11-17.md').write_text('\n'.join(summary+table)+'\n')
    print(dict(counts))


if __name__ == '__main__':
    main()
