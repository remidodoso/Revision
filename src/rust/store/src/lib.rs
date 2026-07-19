//! rev-store — store-primary persistence: the SQLite project store, command
//! journal, and snapshots (R-201..R-205). The journal is the only write path;
//! gesture = transaction. Crash-only by design: kill -9 at any moment loses
//! no committed gesture (the TMON test, R-202/R-808/R-1504). The realization
//! view is the model's executable specification (two-tier strategy).
