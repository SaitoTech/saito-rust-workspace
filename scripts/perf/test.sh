#!/bin/bash

 intermediate_host="deployment.saito.network"
spammer_node_ip="178.128.84.14"
echo "Checking status of spammer_node on $spammer_node_ip via $intermediate_host"
STATUS=$(ssh -t "root@$intermediate_host" "ssh -t root@$spammer_node_ip 'pm2 show spammer_node | grep \"status\" | awk \"{print \$4}\"'")
echo "Status: $STATUS"