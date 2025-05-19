UPDATE filters SET last_observed_block = last_observed_block + $1 WHERE filter_id=$2;
