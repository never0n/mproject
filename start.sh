#!/usr/bin/env bash
set -Eeuo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
mkdir -p .local/logs

for tool in docker cargo curl; do
    command -v "$tool" >/dev/null || { echo "Не найдена команда: $tool" >&2; exit 1; }
done

echo 'Запускаем PostgreSQL…'
if docker info >/dev/null 2>&1; then
    docker compose up -d --wait db
else
    echo 'Для запуска Docker может потребоваться пароль администратора.'
    sudo docker compose up -d --wait db
fi

echo 'Проверяем сборку…'
cargo build --workspace --locked

# The launcher uses the project's local development configuration.
health_url=http://127.0.0.1:3000/health
if response=$(curl --silent --fail --max-time 2 "$health_url"); then
    if [[ "$response" != mortown-* ]]; then
        echo 'Порт 3000 занят другим сервисом. Освободите его перед запуском.' >&2
        exit 1
    fi
    # Compare the running executable with this build, including replaced binaries.
    server_binary="$PWD/target/debug/mortown-server"
    matching_pids=()
    for executable in /proc/[0-9]*/exe; do
        running_path=$(readlink "$executable" 2>/dev/null) || continue
        if [[ "$running_path" == "$server_binary" || "$running_path" == "$server_binary (deleted)" ]]; then
            process_dir=${executable%/exe}
            matching_pids+=("${process_dir##*/}")
        fi
    done
    if (( ${#matching_pids[@]} != 1 )); then
        echo 'Не удалось однозначно определить сервер этой сборки. Освободите порт 3000 и повторите запуск.' >&2
        exit 1
    fi
    server_pid=${matching_pids[0]}
    if ! cmp --silent "/proc/$server_pid/exe" "$server_binary"; then
        echo 'Обновляем работающий сервер; сохраняем персонажей…'
        kill -INT "$server_pid"
        for ((attempt=0; attempt<60; attempt++)); do
            if ! kill -0 "$server_pid" 2>/dev/null; then break; fi
            sleep 0.5
        done
        if kill -0 "$server_pid" 2>/dev/null; then
            echo 'Сервер ещё сохраняет данные. Повторите запуск после его завершения.' >&2
            exit 1
        fi
        response=''
    else
        echo 'Сервер актуальной сборки уже работает.'
    fi
else
    response=''
fi
if [[ -z "$response" ]]; then
    echo 'Запускаем сервер…'
    nohup env DATABASE_URL=postgres://mortown:mortown_local_dev@127.0.0.1:55432/mortown \
        MORTOWN_LISTEN=127.0.0.1:3000 ./target/debug/mortown-server \
        > .local/logs/server.log 2>&1 < /dev/null &
    server_pid=$!
    ready=false
    for ((attempt=0; attempt<60; attempt++)); do
        if ! kill -0 "$server_pid" 2>/dev/null; then
            echo 'Не удалось запустить сервер. Подробности: .local/logs/server.log' >&2
            exit 1
        fi
        if response=$(curl --silent --fail --max-time 1 "$health_url") && [[ "$response" == mortown-* ]]; then
            ready=true
            break
        fi
        sleep 0.5
    done
    if [[ "$ready" != true ]]; then
        kill -INT "$server_pid" 2>/dev/null || true
        echo 'Сервер не ответил. Подробности: .local/logs/server.log' >&2
        exit 1
    fi
fi

echo 'Открываем игру. После закрытия клиента база и сервер продолжат работать.'
env -u MORTOWN_LOGIN -u MORTOWN_PASSWORD MORTOWN_SERVER=ws://127.0.0.1:3000/world \
    ./target/debug/mortown-client > .local/logs/client.log 2>&1 || {
        echo 'Ошибка клиента. Подробности: .local/logs/client.log' >&2
        exit 1
    }
