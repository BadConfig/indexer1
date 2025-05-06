# Changelog
---

## Version: 0.2.11
## Date: 05-07-2025
## Title: race safety

### Added race safetry to database. 
Now if couple of indexers will be run simultaniously on the same data, one of them will fail updating, keeping indexer consistent.
This is reached by adding from block check, where ```SELECT _____ FOR UPDATE``` is used as as query to prevent races.

---

## Version: 0.2.0
## Date: 03-02-2025
## Title: rework storage traits

### Added generic method for ```LogStorage``` trait. 
Now it's possible to implement storages that use their own data type for transaction.
*Note that ```LogStorage::Transaction``` is passed into ```Processor``` as ```&mut LogStorage::Transaction```*

### ```Processor``` now also receives previous and new committed block numbers as arguments
Now it's possible to creatie storages that will implement atomic commitements to remote storages.
You have guarantee that no block sequences will be skipped, means that remote storage should keep track on block number to deduplicate already appended log batches.

---
