# V11 ロールバック — compression_metrics

計測専用テーブル。ドロップしてもメモリ本体（event_log / state / threads / sessions）に影響なし。

```sql
DROP INDEX IF EXISTS idx_compression_metrics_ts;
DROP TABLE IF EXISTS compression_metrics;
```
