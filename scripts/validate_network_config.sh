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
NETWORK_TYPE=`echo ${1} | jq -r '.network_type // "EVM"' | tr '[:upper:]' '[:lower:]'` # Convert to lowercase using tr

echo "Testing RPCs for ${NETWORK_NAME}"

for u in `echo ${1} | jq '.rpc_urls[] | .url.value' | tr -d '"'`
    do
        URL=`echo ${u} | tr -d '"'`

        # Set the method based on network type
        case ${NETWORK_TYPE} in # Network type is already lowercase
            "evm")
                METHOD="net_version"
                ;;
            "stellar")
                METHOD="getNetwork"
                ;;
            "midnight")
                METHOD="system_chain"
                ;;
            *)
                METHOD="net_version"
                ;;
        esac

        # Store the response in a variable and check both HTTP status and JSON response
        RESPONSE=$(curl -s -w "\n%{http_code}" ${URL} -X POST -H "Content-Type: application/json" \
            --data "{\"method\":\"${METHOD}\",\"params\":[],\"id\":1,\"jsonrpc\":\"2.0\"}")

        # Get HTTP status code (last line)
        HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
        # Get response body (all but last line)
        BODY=$(echo "$RESPONSE" | sed \$d)

        # Check both HTTP status and valid JSON response
        if [ $HTTP_STATUS -eq 200 ] && echo "$BODY" | jq empty > /dev/null 2>&1; then
            summary_array+=("✅ RPC ${URL} (${NETWORK_NAME}).")
        else
            summary_array+=("❌ RPC ${URL} (${NETWORK_NAME}).")
            RET_CODE=1
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
