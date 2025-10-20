# Instance ID para Logging

## O que é

Cada instância de `Manager` agora possui um **instance_id** único (primeiros 8 caracteres de um UUID v4), automaticamente gerado quando o Manager é criado. Este ID aparece em todos os logs relacionados àquela instância específica.

## Por que usar

Quando você tem múltiplas conexões AMI simultâneas (por exemplo, conectando a diferentes servidores Asterisk ou mantendo múltiplas conexões ao mesmo servidor), o instance_id permite distinguir claramente qual Manager está gerando cada log.

## Onde aparece nos logs

O instance_id aparece em logs de:

### Manager básico
- Criação: `Creating new Manager instance [abc12345]`

### Heartbeat
- Início: `[abc12345] Starting heartbeat task (interval=30s)`
- Tick: `[abc12345] Heartbeat ping successful`
- Falha: `[abc12345] Heartbeat ping failed: <erro>`
- Cancelamento: `[abc12345] Heartbeat task cancelled`

### Watchdog
- Início: `[abc12345] Starting watchdog (default interval=1s) for user 'admin' at 127.0.0.1:5038`
- Tentativa: `[abc12345] Watchdog attempting reconnection to 'admin'@127.0.0.1:5038...`
- Sucesso: `[abc12345] Watchdog reconnection successful to 'admin'@127.0.0.1:5038`
- Falha: `[abc12345] Watchdog reconnection to 'admin'@127.0.0.1:5038 failed: <erro>`
- Cancelamento: `[abc12345] Watchdog task cancelled by token`

### Resilient module
- Conexão: `[abc12345] Connecting resilient AMI manager to 'admin'@127.0.0.1:5038`
- Stream: `Creating new resilient stream instance [def67890]` (streams têm seus próprios IDs)

## Exemplos práticos

### Logs de uma única conexão
```
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [a1b2c3d4]
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Starting heartbeat task (interval=30s)
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Heartbeat ping successful
```

### Logs de múltiplas conexões simultâneas
```
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [a1b2c3d4]
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Connecting resilient AMI manager to 'admin'@127.0.0.1:5038
[2025-10-20 ... DEBUG asterisk_manager] Creating new Manager instance [e5f6g7h8]
[2025-10-20 ... DEBUG asterisk_manager] [e5f6g7h8] Connecting resilient AMI manager to 'admin2'@127.0.0.1:5039
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Heartbeat ping successful
[2025-10-20 ... DEBUG asterisk_manager] [e5f6g7h8] Watchdog attempting reconnection to 'admin2'@127.0.0.1:5039...
[2025-10-20 ... DEBUG asterisk_manager] [a1b2c3d4] Watchdog tick: already authenticated; no action taken
```

Neste exemplo, você pode ver claramente que:
- `[a1b2c3d4]` é a primeira conexão (porta 5038)
- `[e5f6g7h8]` é a segunda conexão (porta 5039)

## Como usar nos seus logs

O instance_id é especialmente útil quando:

1. **Depurando múltiplas conexões**: Rapidamente identifique qual conexão está com problema
2. **Monitoramento**: Acompanhe métricas específicas por conexão
3. **Troubleshooting**: Rastreie o ciclo de vida completo de uma conexão específica
4. **Auditing**: Correlacione eventos através do tempo para uma única instância

## Exemplos incluídos

- `watchdog_resilient_logging.rs`: Mostra instance_id em uma única conexão
- `multiple_managers_logging.rs`: Demonstra instance_id com múltiplas conexões simultâneas

## Habilitando logs detalhados

Para ver todos os logs com instance_id:

```bash
# Logs debug de todas as operações
RUST_LOG=asterisk_manager=debug cargo run --example multiple_managers_logging

# Logs trace incluindo ticks do watchdog/heartbeat
RUST_LOG=asterisk_manager=trace cargo run --example multiple_managers_logging
```
