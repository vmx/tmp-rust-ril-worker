all: concat

concat:
	cat ril_consts.js worker_buf.js ril_worker_buf_object.js ril_worker_telephony_request_queue.js ril_worker.js epilogue.js > all.js

run: concat
	spidernode all.js

node: concat
	node all.js
