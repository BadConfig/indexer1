# Changelog
---

## Version: 0.2.15
## Date: 05-19-2025
## Title: Builder improvements and lock rework

### Reworked race safetry. 
Now race safety uses locks that are created via update sql, making it more fitting sql standart

### Added new methods to builder. 
Now its possible to add pre built http/ws clients and an optional block range value

### Added overtake interval. 
A custom interval of sleep that will be used if the current to_block is less then latest block

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
