#!/usr/bin/env python3
"""Generate a live log file that can be followed with tail -f or loghew."""

import random
import datetime
import time
import sys
import signal

OUTPUT = "live.log"

LEVELS = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]
LEVEL_WEIGHTS = [5, 15, 60, 15, 5]

SERVICES = ["api-gateway", "auth-service", "user-service", "payment-service",
            "notification-service", "cache-manager", "db-pool", "scheduler",
            "message-queue", "load-balancer"]

ENDPOINTS = [
    "GET /api/v1/users", "POST /api/v1/users", "GET /api/v1/users/{id}",
    "PUT /api/v1/users/{id}", "DELETE /api/v1/users/{id}",
    "POST /api/v1/auth/login", "POST /api/v1/auth/logout",
    "GET /api/v1/orders", "POST /api/v1/orders", "GET /api/v1/orders/{id}",
    "POST /api/v1/payments", "GET /api/v1/payments/{id}",
    "GET /api/v1/products", "GET /api/v1/products/{id}",
    "GET /api/v1/health", "GET /api/v1/metrics",
    "POST /api/v1/notifications/send",
]

TRACE_MSGS = [
    "Entering method processRequest",
    "Exiting method processRequest",
    "Cache key lookup: user:{id}",
    "Serializing response payload",
    "Connection pool stats: active=12 idle=8 total=20",
    "Request body size: {size} bytes",
]

DEBUG_MSGS = [
    "Processing request {req_id} for {endpoint}",
    "Database query executed in {ms}ms: SELECT * FROM users WHERE id = ?",
    "Cache hit for key user:{id} (ttl: {ttl}s remaining)",
    "Cache miss for key session:{id}",
    "JWT token validated for user {id}",
    "Rate limiter: {count}/100 requests in current window",
    "Connection acquired from pool (active: {active}/{total})",
    "Message published to queue '{queue}' (size: {size} bytes)",
]

INFO_MSGS = [
    "Request completed: {endpoint} → 200 OK ({ms}ms)",
    "Request completed: {endpoint} → 201 Created ({ms}ms)",
    "User {id} authenticated successfully from {ip}",
    "Order #{order} created: total=${amount:.2f} items={items}",
    "Payment processed: order #{order} amount=${amount:.2f} via stripe",
    "Email notification sent to user {id}: {template}",
    "Scheduled job '{job}' completed in {ms}ms",
    "Health check passed: all {count} dependencies healthy",
    "Batch job processed {count} records in {ms}ms",
    "File uploaded: {filename} ({size}KB) by user {id}",
]

WARN_MSGS = [
    "Slow query detected ({ms}ms): SELECT * FROM orders WHERE created_at > ?",
    "Rate limit approaching for IP {ip}: {count}/100 requests",
    "Connection pool near capacity: {active}/{total} connections in use",
    "Retry #{n} for downstream call to payment-service ({ms}ms timeout)",
    "Memory usage above threshold: {pct}% of {total_mb}MB heap",
    "Disk space low on /var/log: {pct}% used ({free}GB remaining)",
    "Queue depth increasing: {depth} messages pending ({service})",
    "Response time SLA breach: {endpoint} took {ms}ms (target: 500ms)",
]

ERROR_MSGS = [
    "Request failed: {endpoint} → 500 Internal Server Error",
    "Database connection failed: connection refused (postgresql://db:5432/app)",
    "Authentication failed for user {id}: invalid credentials (attempt {n}/5)",
    "Payment processing failed: Stripe API returned 402 (insufficient_funds)",
    "Timeout waiting for response from {service} after {ms}ms",
    "Circuit breaker OPEN for {service}: {count} failures in last 60s",
    "Invalid JSON in request body: unexpected token at position {pos}",
    "Deadlock detected in database connection pool",
]

STACK_TRACES = [
    [
        "java.lang.NullPointerException: Cannot invoke method on null reference",
        "\tat com.app.service.UserService.getProfile(UserService.java:142)",
        "\tat com.app.controller.UserController.handleGetUser(UserController.java:87)",
        "\tat org.springframework.web.servlet.FrameworkServlet.service(FrameworkServlet.java:897)",
    ],
    [
        "Traceback (most recent call last):",
        '  File "/app/services/user_service.py", line 89, in get_user',
        "    user = db.query(User).filter(User.id == user_id).one()",
        "sqlalchemy.exc.NoResultFound: No row was found when one was required",
    ],
    [
        "thread 'tokio-runtime-worker' panicked at 'called `Option::unwrap()` on a `None` value'",
        "   0: std::panicking::begin_panic_handler",
        "   1: core::panicking::panic_fmt",
        "   2: app::handler::process_request at src/handler.rs:142",
    ],
]

REQUEST_IDS = [f"{random.randint(1000000, 9999999):07x}" for _ in range(100)]
IPS = [f"192.168.{random.randint(1,254)}.{random.randint(1,254)}" for _ in range(30)]
JOBS = ["cleanup-expired-sessions", "send-digest-emails", "aggregate-metrics", "sync-inventory"]
TEMPLATES = ["welcome_email", "password_reset", "order_confirmation", "payment_receipt"]
FILENAMES = ["report_2024.pdf", "avatar.png", "export.csv", "invoice_1234.pdf"]
QUEUES = ["email-notifications", "payment-processing", "order-updates", "analytics-events"]

MSG_MAP = {
    "TRACE": TRACE_MSGS, "DEBUG": DEBUG_MSGS, "INFO": INFO_MSGS,
    "WARN": WARN_MSGS, "ERROR": ERROR_MSGS,
}


def fmt(msg):
    return msg.format(
        id=random.randint(1000, 99999),
        n=random.randint(1, 999),
        req_id=random.choice(REQUEST_IDS),
        endpoint=random.choice(ENDPOINTS),
        ms=random.choice([1, 3, 8, 23, 67, 120, 456, 1200, 5678]),
        ip=random.choice(IPS),
        count=random.randint(1, 500),
        active=random.randint(10, 20),
        total=20,
        order=random.randint(10000, 99999),
        amount=round(random.uniform(5.99, 2499.99), 2),
        items=random.randint(1, 12),
        service=random.choice(SERVICES),
        size=random.randint(10, 5000),
        pct=random.randint(75, 98),
        total_mb=random.choice([512, 1024, 2048, 4096]),
        free=round(random.uniform(0.5, 5.0), 1),
        depth=random.randint(50, 5000),
        ttl=random.randint(10, 3600),
        queue=random.choice(QUEUES),
        pos=random.randint(1, 500),
        template=random.choice(TEMPLATES),
        job=random.choice(JOBS),
        filename=random.choice(FILENAMES),
    )


def write_line(f, line):
    f.write(line + "\n")
    f.flush()


def log_line(level, service, msg):
    ts = datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]
    return f"{ts} [{level:<5}] [{service}] {msg}"


running = True

def handle_signal(sig, frame):
    global running
    running = False

signal.signal(signal.SIGINT, handle_signal)
signal.signal(signal.SIGTERM, handle_signal)


def main():
    output = OUTPUT
    if len(sys.argv) > 1:
        output = sys.argv[1]

    print(f"Writing live logs to {output} (Ctrl+C to stop)")

    with open(output, "w") as f:
        service = "api-gateway"
        for msg in [
            "Application starting: loghew-demo v2.4.1",
            "Loading configuration from /etc/app/config.yaml",
        ]:
            write_line(f, log_line("INFO", service, msg))
        time.sleep(0.05)

        write_line(f, log_line("INFO", "db-pool", "Initializing connection pool: postgresql://db:5432/app (max=20)"))
        time.sleep(0.15)
        write_line(f, log_line("INFO", "db-pool", "Connection pool ready: 20 connections established"))
        write_line(f, log_line("INFO", "cache-manager", "Redis connection established, cluster mode enabled"))
        time.sleep(0.1)
        write_line(f, log_line("INFO", "message-queue", "Queue bindings established: 6 queues, 12 consumers"))
        write_line(f, log_line("INFO", service, "Server listening on 0.0.0.0:8080"))
        write_line(f, log_line("INFO", service, "Application ready — startup completed in 0.3s"))
        time.sleep(0.2)

        burst_counter = 0

        while running:
            burst_counter += 1

            if burst_counter % 200 == 0 and random.random() < 0.2:
                burst_size = random.randint(5, 15)
                for _ in range(burst_size):
                    if not running:
                        break
                    level = random.choices(LEVELS, [1, 5, 15, 35, 44])[0]
                    service = random.choice(SERVICES)
                    msg = fmt(random.choice(MSG_MAP[level]))
                    write_line(f, log_line(level, service, msg))

                    if level == "ERROR" and random.random() < 0.5:
                        for trace_line in random.choice(STACK_TRACES):
                            write_line(f, trace_line)

                    time.sleep(random.uniform(0.01, 0.05))
                continue

            batch = random.randint(1, 3)
            for _ in range(batch):
                if not running:
                    break
                level = random.choices(LEVELS, LEVEL_WEIGHTS)[0]
                service = random.choice(SERVICES)
                msg = fmt(random.choice(MSG_MAP[level]))
                write_line(f, log_line(level, service, msg))

                if level == "ERROR" and random.random() < 0.3:
                    for trace_line in random.choice(STACK_TRACES):
                        write_line(f, trace_line)

            delay = random.choices(
                [0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0],
                [20, 30, 25, 15, 5, 3, 2],
            )[0]
            time.sleep(delay)

        write_line(f, log_line("INFO", "api-gateway", "Received SIGTERM, initiating graceful shutdown"))
        time.sleep(0.1)
        write_line(f, log_line("INFO", "api-gateway", "Stopping HTTP listener, draining active connections..."))
        time.sleep(0.3)
        write_line(f, log_line("INFO", "db-pool", "Connection pool drained: 20 connections closed"))
        write_line(f, log_line("INFO", "api-gateway", "Graceful shutdown completed"))

    print(f"\nStopped. Wrote to {output}")


if __name__ == "__main__":
    main()
