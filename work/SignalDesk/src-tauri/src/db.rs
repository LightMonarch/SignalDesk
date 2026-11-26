use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::path::Path;
type R<T> = rusqlite::Result<T>;
pub struct Store(pub Connection);
impl Store {
    pub fn open(path: &Path) -> R<Self> {
        let c = Connection::open(path)?;
        c.busy_timeout(std::time::Duration::from_secs(4))?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
 CREATE TABLE IF NOT EXISTS series(bucket INTEGER,k TEXT,n INTEGER,lo REAL,hi REAL,total REAL,PRIMARY KEY(bucket,k)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS recent(ts INTEGER PRIMARY KEY,data TEXT);
 CREATE TABLE IF NOT EXISTS records(kind TEXT,id TEXT,ts INTEGER,data TEXT,PRIMARY KEY(kind,id,ts)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS records_time ON records(kind,ts);
 ")?;
        Ok(Self(c))
    }
    pub fn sample(&mut self, ts: i64, m: &Value) -> R<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO recent VALUES(?,?)",
            params![ts, m.to_string()],
        )?;
        {
            let mut stmt=tx.prepare_cached("INSERT INTO series VALUES(?1,?2,1,?3,?3,?3) ON CONFLICT(bucket,k) DO UPDATE SET n=n+1,lo=min(lo,excluded.lo),hi=max(hi,excluded.hi),total=total+excluded.total")?;
            if let Some(map) = m.as_object() {
                for (k, v) in map {
                    if let Some(n) = v.as_f64() {
                        stmt.execute(params![ts / 60 * 60, k, n])?;
                    }
                }
            }
        }
        tx.commit()
    }
    pub fn prune(&self, now: i64, days: u64) -> R<()> {
        let since = now - days as i64 * 86400;
        self.0
            .execute("DELETE FROM recent WHERE ts<?", [now - 900])?;
        self.0
            .execute("DELETE FROM series WHERE bucket<?", [since])?;
        self.0.execute("DELETE FROM records WHERE ts<?", [since])?;
        Ok(())
    }
    pub fn history(&self, start: i64, end: i64, now: i64) -> R<Value> {
        let mut points = std::collections::BTreeMap::<i64, Map<String, Value>>::new();
        if start >= now - 900 {
            let mut q = self
                .0
                .prepare("SELECT ts,data FROM recent WHERE ts BETWEEN ? AND ? ORDER BY ts")?;
            let rows = q.query_map([start, end], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (t, s) = row?;
                if let Ok(m) = serde_json::from_str::<Map<String, Value>>(&s) {
                    let mut p = Map::new();
                    for (k, v) in m {
                        if v.is_number() {
                            p.insert(k, json!({"avg":v,"min":v,"max":v,"n":1}));
                        }
                    }
                    points.insert(t, p);
                }
            }
        } else {
            let width = ((end - start) / 240 / 60).max(1) * 60;
            let mut q=self.0.prepare("SELECT bucket/?3*?3,k,min(lo),max(hi),sum(total)/sum(n),sum(n) FROM series WHERE bucket BETWEEN ?1 AND ?2 GROUP BY 1,2 ORDER BY 1")?;
            let mut rows = q.query(params![start / 60 * 60, end, width])?;
            while let Some(r) = rows.next()? {
                points.entry(r.get(0)?).or_default().insert(r.get(1)?,json!({"min":r.get::<_,f64>(2)?,"max":r.get::<_,f64>(3)?,"avg":r.get::<_,f64>(4)?,"n":r.get::<_,i64>(5)?}));
            }
        }
        Ok(Value::Array(
            points
                .into_iter()
                .map(|(ts, m)| json!({"ts":ts,"values":m}))
                .collect(),
        ))
    }
    pub fn record(&self, kind: &str, id: &str, ts: i64, data: &Value) -> R<()> {
        self.0.execute(
            "INSERT OR REPLACE INTO records VALUES(?,?,?,?)",
            params![kind, id, ts, data.to_string()],
        )?;
        Ok(())
    }
    pub fn last(&self, kind: &str, id: &str) -> R<Option<i64>> {
        self.0.query_row(
            "SELECT max(ts) FROM records WHERE kind=? AND id=?",
            params![kind, id],
            |r| r.get(0),
        )
    }
    pub fn records(&self, kind: &str) -> R<Vec<Value>> {
        let mut q = self
            .0
            .prepare("SELECT data FROM records WHERE kind=? ORDER BY ts DESC LIMIT 1000")?;
        let rows = q.query_map([kind], |r| r.get::<_, String>(0))?;
        let mut v = vec![];
        for r in rows {
            if let Ok(p) = serde_json::from_str(&r?) {
                v.push(p)
            }
        }
        Ok(v)
    }
    pub fn mark(&self, kind: &str, id: &str, ts: i64, status: &str) -> R<()> {
        self.0.execute("UPDATE records SET data=json_set(data,'$.status',?1) WHERE kind=?2 AND id=?3 AND ts=?4",params![status,kind,id,ts])?;
        Ok(())
    }
    pub fn clear(&mut self) -> R<()> {
        let tx = self.0.transaction()?;
        tx.execute_batch("DELETE FROM recent; DELETE FROM series; DELETE FROM records;")?;
        tx.commit()?;
        self.0.execute_batch("VACUUM")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregates_preserve_extremes() {
        let mut d = Store::open(Path::new(":memory:")).unwrap();
        d.sample(6000, &json!({"cpu":0,"gpu":null})).unwrap();
        d.sample(6010, &json!({"cpu":100})).unwrap();
        let h = d.history(0, 7000, 8000).unwrap();
        assert_eq!(h[0]["values"]["cpu"]["avg"], 50.);
        assert_eq!(h[0]["values"]["cpu"]["min"], 0.);
        assert_eq!(h[0]["values"]["cpu"]["max"], 100.);
        assert!(h[0]["values"]["gpu"].is_null());
        d.clear().unwrap();
        assert!(d
            .history(0, 8000, 8000)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
}
