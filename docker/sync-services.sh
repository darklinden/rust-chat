#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
PROJECT_DIR="$(realpath "${BASEDIR}")"

# SERVER_HOST=""
# REMOTE_PATH=""

echo "Load environment variables ..."

if [ -f $PROJECT_DIR/.env ]; then
    echo "Loading environment variables from $PROJECT_DIR/.env ..."
    set -a
    source $PROJECT_DIR/.env
    set +a
fi

echo "SERVER_HOST: $SERVER_HOST"
echo "REMOTE_PATH: $REMOTE_PATH"

# countdown timer
function countdown() {
    secs=$1
    shift
    msg=$@
    while [ $secs -gt 0 ]; do
        printf "\r\033[K $msg %02d" $((secs--))
        sleep 1
    done
    echo
}

echo "Will Sync Services ..."
countdown 3 "Stat In"

echo "Network Sync ..."

echo "Source Directory $PROJECT_DIR"
echo "Destination Directory $SERVER_HOST:$REMOTE_PATH"

rsync -av $PROJECT_DIR/ $SERVER_HOST:$REMOTE_PATH --exclude-from='exclude.txt'
