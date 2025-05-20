UPDATE filters SET last_observed_block = $1 WHERE filter_id=$3 and last_observed_block = $2;
