#!/bin/bash

FOLDER_PATH="./config/networks" # Change to your folder path
RET_CODE=0

declare -a json_array
declare -a summary_array

#
#  $1 -> json schema for the network configuration (./config/networks/)
#
function test_rpcs {
NETWORK_NAME=`echo ${1} | jq '.name'`
       
echo "Testing RPCs for ${NETWORK_NAME}"

for u in `echo ${1} | jq '.rpc_urls[] | .url' | tr -d '"'`
    do
        URL=`echo ${u} | tr -d '"'`
        curl ${URL} -s -X POST -H "Content-Type: application/json" --data '{"method":"net_version","params":[],"id":1, "jsonrpc":"2.0"}'>/dev/null
        if [ $? -ne 0 ]
        then
            summary_array+=("ERROR: Check failed for RPC ${URL} (${NETWORK_NAME}).")
            RET_CODE=1
        else
        summary_array+=("SUCCESS: RPC ${URL} (${NETWORK_NAME}).")
    fi
done
}

# parsing arguments (if any)
while getopts :hf: opt; do
    case ${opt} in
        h)
	    echo "Usage: $0 [-h | -f <directory to check> ]"
	    exit 0
	    ;;
        f)
            FOLDER_PATH=${OPTARG}
	    ;;
	:)
	    echo "Option -${OPTARG} requires an argument"
	    exit 1
	    ;;
    esac
done	

if [ -d "$FOLDER_PATH" ]; then
    for file in "$FOLDER_PATH"/*.json*; do
        if [ -f "$file" ]; then
            content=$(cat "$file")
            json_array+=("$content")
        fi
    done

    echo "Loaded ${#json_array[@]} JSON files from ${FOLDER_PATH}"

    for i in "${json_array[@]}"
    do
        test_rpcs "${i}"
    done
else
    echo "Folder not found: $FOLDER_PATH"
fi

for i in "${summary_array[@]}"
do
    echo ${i}
done

exit $RET_CODE
