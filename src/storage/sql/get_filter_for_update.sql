SELECT last_observed_block FROM filters WHERE filter_id=$1 FOR UPDATE;
