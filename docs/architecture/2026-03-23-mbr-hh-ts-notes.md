# GG MBR HH/TS: зафиксированные форматные наблюдения

## Источник

- HH ZIP: `D:/Downloads/0000019d-18a9-8d28-0000-0000ec0c5e73.zip`
- TS ZIP: `D:/Downloads/0000019d-18a9-6402-0000-0000ec0c5e73.zip`

## Sample pack

В архивах лежат 9 турниров GG MBR за `2026-03-16`.

| Tournament ID | HH file | TS file | Finish | Total payout | Есть 9-max FT |
|---|---|---|---:|---:|---|
| `271767530` | `GG20260316-0307 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt` | 10 | 0 | нет |
| `271767841` | `GG20260316-0312 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271767841 - Mystery Battle Royale 25.txt` | 5 | 0 | да |
| `271768265` | `GG20260316-0316 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271768265 - Mystery Battle Royale 25.txt` | 5 | 13 | да |
| `271768505` | `GG20260316-0319 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271768505 - Mystery Battle Royale 25.txt` | 5 | 0 | да |
| `271768917` | `GG20260316-0323 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271768917 - Mystery Battle Royale 25.txt` | 1 | 142 | да |
| `271769484` | `GG20260316-0338 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt` | 6 | 0 | да |
| `271769772` | `GG20260316-0342 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt` | 4 | 0 | да |
| `271770266` | `GG20260316-0344 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt` | 1 | 205 | да |
| `271771269` | `GG20260316-0351 - Mystery Battle Royale 25.txt` | `GG20260316 - Tournament #271771269 - Mystery Battle Royale 25.txt` | 6 | 0 | да |

## HH: что точно видно по формату

### 1. Один HH-файл = один турнир

- Внутри файла все раздачи относятся к одному `Tournament #...`.
- Tournament ID берётся из заголовка каждой руки.

### 2. HH идут в обратной хронологии

- Первой в файле стоит более поздняя раздача.
- Для корректного timeline раздачи нужно разворачивать в chronological order.
- Это критично для поиска первой FT-раздачи и boundary-перехода.

### 3. Rush стадия = `5-max`, FT стадия = `9-max`

- Rush-раздачи помечены как `Table 'X' 5-max`.
- FT-раздачи помечены как `Table 'X' 9-max`.
- При этом FT может стартовать short-handed: строка стола остаётся `9-max`, но фактически в раздаче может быть 6-7 игроков и пропущенные номера мест.

### 4. Переход `5-max -> 9-max` виден только косвенно

- Exact-факт: Hero сыграл 9-max раздачу (`played_ft_hand = true`).
- Не-exact факт: Hero вошёл в последнюю rush-волну, из которой мог возникнуть boundary KO (`entered_boundary_zone`).
- По hero-only HH нельзя восстановить точное число живых игроков в момент выбивания на границе Rush -> FT.

### 5. Формат заголовка руки

Пример:

```text
Poker Hand #BR1064994395: Tournament #271768505, Mystery Battle Royale $25 Hold'em No Limit - Level10(200/400(80)) - 2026/03/16 10:42:18
```

Из заголовка нужно стабильно доставать:

- `external_hand_id = BR...`
- `external_tournament_id`
- турнирный формат / buy-in label
- `level`
- `small_blind`
- `big_blind`
- `ante`
- `hand_started_at`

### 6. Seat map неполный и с пропусками

- На FT возможны пропущенные seat numbers.
- Пустые места не перечисляются.
- Значит seat set нужно строить только по фактическим строкам `Seat N:`.

### 7. Hero-only visibility

- `Dealt to Hero [.. ..]` содержит hole cards только Hero.
- Для остальных игроков в начале руки карты не раскрыты.
- Карты оппонентов доступны только через `shows` в action block и/или в summary showdown строках.

### 8. Основные action-формы

Во встреченных примерах есть:

- `posts the ante`
- `posts small blind`
- `posts big blind`
- `folds`
- `checks`
- `calls N`
- `calls N and is all-in`
- `raises X to Y`
- `raises X to Y and is all-in`
- `bets N`
- `bets N and is all-in`
- `shows [.. ..] (...)`

Парсер должен хранить и raw amount, и `to_amount` для `raise-to`.

### 9. Uncalled returns обязательны для корректной нормализации

Примеры:

- `Uncalled bet (557) returned to d36b0ae5`
- `Uncalled bet (2,270) returned to 4b4890a7`

Без этого будет ломаться:

- финальный committed amount;
- построение side pots;
- расчёт final stacks;
- chip conservation.

### 10. В одной руке может быть несколько `collected ... from pot`

Это не редкий крайний кейс, а реальный формат sample pack.

Пример:

- одна и та же раздача может содержать несколько строк `collected`, в том числе несколькими победителями;
- summary ниже при этом показывает уже агрегированную сумму выигрыша игрока.

Следствие:

- `hand_pots` и `hand_pot_winners` обязательны;
- нельзя хранить только “общего победителя руки”;
- нужны separate `pot_no`, `share_amount`, split-pot поддержка.

### 11. `*** SHOWDOWN ***` присутствует даже в no-flop / no-showdown сценариях

- Наличие маркера `SHOWDOWN` нельзя использовать как доказательство реального вскрытия.
- Нужно опираться на action/show lines и summary.

### 12. Summary несёт дополнительную нормализующую информацию

Из summary берутся:

- `Total pot`
- `Board [...]`, если борд был открыт
- финальный текст по каждому месту:
  - `folded before Flop`
  - `folded on the Turn/River`
  - `showed [...] and won (...)`
  - `collected (...)`

Summary полезен для:

- showdown cards;
- итоговых winners;
- финальной hand-class строки;
- места button / blind marker в seat line.

## TS: что точно видно по формату

Пример:

```text
Tournament #271768505, Mystery Battle Royale $25, Hold'em No Limit
Buy-in: $12.5+$2+$10.5
18 Players
Total Prize Pool: $414
Tournament started 2026/03/16 10:19:41
5th : Hero, $0
You finished the tournament in 5th place.
You received a total of $0.
```

### Из TS можно стабильно достать

- `external_tournament_id`
- tournament title
- buy-in decomposition
- max players = `18`
- started_at
- finish_place
- `total_payout_money`

### Из TS нельзя достать напрямую

- regular prize component по месту;
- mystery bounty total отдельно от regular prize;
- состав KO-конвертов;
- количество KO-событий;
- точный FT boundary state.

Следствие:

- `mystery_money_total = total_payout_money - regular_prize(place)` требует отдельной prize table логики;
- Big KO decoder обязан работать как derived-слой поверх TS + eliminations, а не как прямой parse step.

## Зафиксированные parser edge cases для новой реализации

1. Хронология в HH обратная, парсер обязан нормализовать порядок рук.
2. Переход к FT определяется по первой хронологической `9-max` руке, а не по позиции файла.
3. Первая FT-рука может быть short-handed.
4. Hero видит hole cards только у себя и частично на шоудауне.
5. Seat numbers могут быть дырявыми.
6. `raise-to` надо хранить отдельно от increment.
7. `uncalled bet` обязателен для корректной conservation-модели.
8. Одна рука может содержать несколько pots и нескольких winners.
9. Summary и action block дают разную гранулярность данных, нужен merge в canonical model.
10. Hero-only HH не позволяет exact boundary KO, поэтому нужен estimated resolver из ТЗ.

## Практический вывод для БД и парсера

- Нужен raw/source слой (`source_files`, `file_fragments`, `parse_issues`).
- Нужна полноценная canonical hand model, а не только “финальные руки” или старый `hero_final_table_hands`.
- Нужен normalizer с потами, вкладами, возвратами и final stacks.
- Нужен отдельный `derived.mbr_stage_resolution`, где `played_ft_hand` и `entered_boundary_zone` разделены.
- Для Big KO требуется хранить eliminations, split factor и tournament-level payout facts.
