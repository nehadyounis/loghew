#!/usr/bin/env python3
"""Generate a realistic application log file for testing LogHew."""

import random
import datetime
import sys

random.seed(42)

LEVELS = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]
LEVEL_WEIGHTS = [5, 15, 60, 15, 5]

SERVICES = ["api-gateway", "auth-service", "user-service", "payment-service",
            "notification-service", "cache-manager", "db-pool", "scheduler",
            "message-queue", "load-balancer"]

REQUEST_IDS = [f"{random.randint(1000000, 9999999):07x}" for _ in range(200)]

ENDPOINTS = [
    "GET /api/v1/users", "POST /api/v1/users", "GET /api/v1/users/{id}",
    "PUT /api/v1/users/{id}", "DELETE /api/v1/users/{id}",
    "POST /api/v1/auth/login", "POST /api/v1/auth/logout", "POST /api/v1/auth/refresh",
    "GET /api/v1/orders", "POST /api/v1/orders", "GET /api/v1/orders/{id}",
    "POST /api/v1/payments", "GET /api/v1/payments/{id}",
    "GET /api/v1/products", "GET /api/v1/products/{id}",
    "GET /api/v1/health", "GET /api/v1/metrics",
    "POST /api/v1/notifications/send", "GET /api/v1/notifications",
    "POST /api/v1/webhooks",
]

TRACE_MSGS = [
    "Entering method processRequest",
    "Exiting method processRequest",
    "Cache key lookup: user:{id}",
    "Serializing response payload",
    "Deserializing request body",
    "Connection pool stats: active=12 idle=8 total=20",
    "Header validation passed",
    "Request body size: {size} bytes",
    "Response serialization took {ms}ms",
    "Middleware chain: [auth, rate-limit, logging, cors]",
]

DEBUG_MSGS = [
    "Processing request {req_id} for {endpoint}",
    "Database query executed in {ms}ms: SELECT * FROM users WHERE id = ?",
    "Cache hit for key user:{id} (ttl: {ttl}s remaining)",
    "Cache miss for key session:{id}",
    "JWT token validated for user {id}",
    "Rate limiter: {count}/100 requests in current window",
    "Connection acquired from pool (active: {active}/{total})",
    "Scheduling task {task} for execution in {delay}ms",
    "Message published to queue '{queue}' (size: {size} bytes)",
    "TLS handshake completed with cipher TLS_AES_256_GCM_SHA384",
    "DNS resolved api.stripe.com to 52.18.{a}.{b} in {ms}ms",
    "Retry attempt {n}/3 for request {req_id}",
]

INFO_MSGS = [
    "Request completed: {endpoint} → 200 OK ({ms}ms)",
    "Request completed: {endpoint} → 201 Created ({ms}ms)",
    "Request completed: {endpoint} → 204 No Content ({ms}ms)",
    "User {id} authenticated successfully from {ip}",
    "New user registered: user_id={id} email=user{n}@example.com",
    "Order #{order} created: total=${amount:.2f} items={items}",
    "Payment processed: order #{order} amount=${amount:.2f} via stripe",
    "Email notification sent to user {id}: {template}",
    "Scheduled job '{job}' completed in {ms}ms",
    "Health check passed: all {count} dependencies healthy",
    "Configuration reloaded from /etc/app/config.yaml",
    "Server listening on 0.0.0.0:{port}",
    "Graceful shutdown initiated",
    "Worker thread pool size adjusted: {old} → {new}",
    "Database migration applied: v{major}.{minor}.{patch}",
    "Cache warmed up: {count} entries loaded in {ms}ms",
    "File uploaded: {filename} ({size}KB) by user {id}",
    "WebSocket connection established: client_id={id}",
    "Batch job processed {count} records in {ms}ms",
    "SSL certificate valid until 2027-03-15",
]

WARN_MSGS = [
    "Slow query detected ({ms}ms): SELECT * FROM orders WHERE created_at > ?",
    "Rate limit approaching for IP {ip}: {count}/100 requests",
    "Connection pool near capacity: {active}/{total} connections in use",
    "Retry #{n} for downstream call to payment-service ({ms}ms timeout)",
    "Deprecated API endpoint called: GET /api/v0/users (client: {agent})",
    "Memory usage above threshold: {pct}% of {total}MB heap",
    "Disk space low on /var/log: {pct}% used ({free}GB remaining)",
    "Request payload exceeds recommended size: {size}KB (limit: 1024KB)",
    "Session about to expire for user {id} (2 minutes remaining)",
    "Certificate expiry warning: TLS cert expires in {days} days",
    "Unrecognized header 'X-Custom-{name}' in request from {ip}",
    "Queue depth increasing: {depth} messages pending ({service})",
    "Failover triggered: primary database unreachable, switching to replica",
    "Response time SLA breach: {endpoint} took {ms}ms (target: 500ms)",
]

ERROR_MSGS = [
    "Request failed: {endpoint} → 500 Internal Server Error",
    "Database connection failed: connection refused (postgresql://db:5432/app)",
    "Authentication failed for user {id}: invalid credentials (attempt {n}/5)",
    "Payment processing failed: Stripe API returned 402 (insufficient_funds)",
    "Timeout waiting for response from {service} after {ms}ms",
    "Failed to send notification to user {id}: SMTP connection refused",
    "Out of memory: unable to allocate {size}MB for request buffer",
    "Unhandled exception in request handler",
    "Circuit breaker OPEN for {service}: {count} failures in last 60s",
    "Failed to write to audit log: permission denied on /var/log/audit.log",
    "Message queue connection lost: rabbitmq://mq:5672 (ECONNRESET)",
    "Invalid JSON in request body: unexpected token at position {pos}",
    "SSL handshake failed: certificate verification error",
    "Deadlock detected in database connection pool",
]

JAVA_STACK_TRACES = [
    [
        "java.lang.NullPointerException: Cannot invoke method on null reference",
        "\tat com.app.service.UserService.getProfile(UserService.java:142)",
        "\tat com.app.controller.UserController.handleGetUser(UserController.java:87)",
        "\tat com.app.middleware.AuthMiddleware.doFilter(AuthMiddleware.java:56)",
        "\tat org.springframework.web.servlet.FrameworkServlet.service(FrameworkServlet.java:897)",
        "\tat javax.servlet.http.HttpServlet.service(HttpServlet.java:750)",
        "\tat org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)",
        "\tat org.apache.catalina.core.StandardWrapperValve.invoke(StandardWrapperValve.java:199)",
    ],
    [
        "java.sql.SQLException: Connection pool exhausted (max=20, active=20, waiting=5)",
        "\tat com.zaxxer.hikari.pool.HikariPool.getConnection(HikariPool.java:155)",
        "\tat com.zaxxer.hikari.HikariDataSource.getConnection(HikariDataSource.java:112)",
        "\tat com.app.repository.OrderRepository.findByUserId(OrderRepository.java:78)",
        "\tat com.app.service.OrderService.getUserOrders(OrderService.java:234)",
        "\tat com.app.controller.OrderController.listOrders(OrderController.java:45)",
    ],
    [
        "java.util.concurrent.TimeoutException: Timed out waiting for response after 30000ms",
        "\tat com.app.client.PaymentClient.charge(PaymentClient.java:89)",
        "\tat com.app.service.PaymentService.processPayment(PaymentService.java:156)",
        "\tat com.app.service.OrderService.finalizeOrder(OrderService.java:312)",
        "\tat com.app.controller.OrderController.createOrder(OrderController.java:78)",
        "\tat sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
        "\tat org.springframework.web.method.support.InvocableHandlerMethod.doInvoke(InvocableHandlerMethod.java:190)",
    ],
    [
        "com.fasterxml.jackson.core.JsonParseException: Unexpected character ('x' (code 120)): was expecting double-quote to start field name",
        "\tat com.fasterxml.jackson.core.JsonParser._constructError(JsonParser.java:1851)",
        "\tat com.fasterxml.jackson.core.base.ParserMinimalBase._reportUnexpectedChar(ParserMinimalBase.java:594)",
        "\tat com.fasterxml.jackson.databind.ObjectMapper.readValue(ObjectMapper.java:3478)",
        "\tat com.app.middleware.RequestParser.parseBody(RequestParser.java:67)",
    ],
    [
        "redis.clients.jedis.exceptions.JedisConnectionException: Could not get a resource from the pool",
        "\tat redis.clients.jedis.util.Pool.getResource(Pool.java:84)",
        "\tat com.app.cache.RedisCache.get(RedisCache.java:45)",
        "\tat com.app.service.SessionService.getSession(SessionService.java:112)",
        "\tat com.app.middleware.AuthMiddleware.authenticate(AuthMiddleware.java:34)",
    ],
]

RUST_PANICS = [
    [
        "thread 'tokio-runtime-worker' panicked at 'called `Option::unwrap()` on a `None` value'",
        "   0: std::panicking::begin_panic_handler",
        "   1: core::panicking::panic_fmt",
        "   2: core::panicking::panic",
        "   3: core::option::Option<T>::unwrap",
        "   4: app::handler::process_request",
        "             at src/handler.rs:142",
        "   5: app::server::handle_connection",
        "             at src/server.rs:87",
        "   6: tokio::runtime::task::core::Core<T,S>::poll",
        "   7: tokio::runtime::task::harness::Harness<T,S>::poll",
    ],
]

PYTHON_TRACEBACKS = [
    [
        "Traceback (most recent call last):",
        '  File "/app/services/user_service.py", line 89, in get_user',
        "    user = db.query(User).filter(User.id == user_id).one()",
        '  File "/usr/lib/python3.11/site-packages/sqlalchemy/orm/query.py", line 2824, in one',
        "    raise NoResultFound('No row was found when one was required')",
        "sqlalchemy.exc.NoResultFound: No row was found when one was required",
    ],
    [
        "Traceback (most recent call last):",
        '  File "/app/workers/email_worker.py", line 45, in send_email',
        "    smtp.sendmail(from_addr, to_addr, msg.as_string())",
        '  File "/usr/lib/python3.11/smtplib.py", line 885, in sendmail',
        "    raise SMTPRecipientsRefused(senderrs)",
        "smtplib.SMTPRecipientsRefused: {'user@invalid.example': (550, b'5.1.1 The email account does not exist')}",
    ],
]

IPS = [f"192.168.{random.randint(1,254)}.{random.randint(1,254)}" for _ in range(50)] + \
      [f"10.0.{random.randint(1,254)}.{random.randint(1,254)}" for _ in range(30)] + \
      [f"172.16.{random.randint(1,254)}.{random.randint(1,254)}" for _ in range(20)]

AGENTS = ["curl/7.88.1", "PostmanRuntime/7.32.3", "python-requests/2.31.0",
          "Mozilla/5.0", "okhttp/4.12.0", "Go-http-client/2.0"]

JOBS = ["cleanup-expired-sessions", "send-digest-emails", "aggregate-metrics",
        "sync-inventory", "generate-reports", "rotate-logs", "backup-database"]

TEMPLATES = ["welcome_email", "password_reset", "order_confirmation",
             "shipping_notification", "payment_receipt", "account_deactivation"]

FILENAMES = ["report_2024.pdf", "avatar.png", "export.csv", "invoice_1234.pdf",
             "backup.tar.gz", "screenshot.jpg", "data.json"]

QUEUES = ["email-notifications", "payment-processing", "order-updates",
          "analytics-events", "audit-log", "webhook-delivery"]

def fmt(msg):
    return msg.format(
        id=random.randint(1000, 99999),
        n=random.randint(1, 999),
        req_id=random.choice(REQUEST_IDS),
        endpoint=random.choice(ENDPOINTS),
        ms=random.choice([1, 2, 3, 5, 8, 12, 23, 45, 67, 120, 234, 456, 789, 1200, 2345, 5678, 12340]),
        ip=random.choice(IPS),
        count=random.randint(1, 500),
        active=random.randint(10, 20),
        total=20,
        order=random.randint(10000, 99999),
        amount=round(random.uniform(5.99, 2499.99), 2),
        items=random.randint(1, 12),
        service=random.choice(SERVICES),
        port=random.choice([8080, 8443, 3000, 9090]),
        old=random.randint(4, 8),
        new=random.randint(8, 16),
        major=random.randint(1, 3),
        minor=random.randint(0, 15),
        patch=random.randint(0, 30),
        filename=random.choice(FILENAMES),
        size=random.randint(10, 5000),
        pct=random.randint(75, 98),
        free=round(random.uniform(0.5, 5.0), 1),
        days=random.randint(7, 30),
        agent=random.choice(AGENTS),
        depth=random.randint(50, 5000),
        ttl=random.randint(10, 3600),
        task=f"task-{random.randint(1000,9999)}",
        delay=random.randint(100, 30000),
        queue=random.choice(QUEUES),
        name=random.choice(["Trace", "Forward", "Debug", "Auth"]),
        pos=random.randint(1, 500),
        a=random.randint(1, 254),
        b=random.randint(1, 254),
        template=random.choice(TEMPLATES),
        job=random.choice(JOBS),
    )

def get_msg(level):
    if level == "TRACE":
        return fmt(random.choice(TRACE_MSGS))
    elif level == "DEBUG":
        return fmt(random.choice(DEBUG_MSGS))
    elif level == "INFO":
        return fmt(random.choice(INFO_MSGS))
    elif level == "WARN":
        return fmt(random.choice(WARN_MSGS))
    elif level == "ERROR":
        return fmt(random.choice(ERROR_MSGS))

def main():
    target_gb = float(sys.argv[1]) if len(sys.argv) > 1 else 0.005
    target_bytes = int(target_gb * 1024 * 1024 * 1024)
    output = "test.log"

    start = datetime.datetime(2024, 11, 15, 6, 0, 0)
    t = start

    startup = [
        (t, "INFO",  "api-gateway", "Application starting: loghew-demo v2.4.1"),
        (t + datetime.timedelta(milliseconds=50), "INFO", "api-gateway", "Loading configuration from /etc/app/config.yaml"),
        (t + datetime.timedelta(milliseconds=120), "INFO", "db-pool", "Initializing connection pool: postgresql://db:5432/app (max=20)"),
        (t + datetime.timedelta(milliseconds=350), "INFO", "db-pool", "Connection pool ready: 20 connections established"),
        (t + datetime.timedelta(milliseconds=400), "INFO", "cache-manager", "Connecting to Redis cluster: redis://cache:6379"),
        (t + datetime.timedelta(milliseconds=520), "INFO", "cache-manager", "Redis connection established, cluster mode enabled"),
        (t + datetime.timedelta(milliseconds=600), "INFO", "cache-manager", "Cache warmed up: 14523 entries loaded in 80ms"),
        (t + datetime.timedelta(milliseconds=700), "INFO", "message-queue", "Connecting to RabbitMQ: amqp://mq:5672"),
        (t + datetime.timedelta(milliseconds=850), "INFO", "message-queue", "Queue bindings established: 6 queues, 12 consumers"),
        (t + datetime.timedelta(seconds=1), "INFO", "scheduler", "Scheduled jobs loaded: 7 jobs registered"),
        (t + datetime.timedelta(seconds=1, milliseconds=100), "INFO", "api-gateway", "Server listening on 0.0.0.0:8080"),
        (t + datetime.timedelta(seconds=1, milliseconds=150), "INFO", "api-gateway", "Server listening on 0.0.0.0:8443 (TLS)"),
        (t + datetime.timedelta(seconds=1, milliseconds=200), "INFO", "api-gateway", "Application ready — startup completed in 1.2s"),
    ]

    print(f"Generating ~{target_gb}GB → {output}")

    written = 0
    line_count = 0
    counts = {l: 0 for l in LEVELS}

    with open(output, "w", buffering=1024*1024) as f:
        for ts, level, service, msg in startup:
            line = f"{ts.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]} [{level:<5}] [{service}] {msg}\n"
            f.write(line)
            written += len(line)
            line_count += 1
            counts["INFO"] += 1

        t = start + datetime.timedelta(seconds=2)
        i = line_count

        # Scatter several incident windows across the file
        incident_zones = set()
        zone_spacing = max(10000, target_bytes // (100 * 100))
        for z in range(1, 20):
            center = z * zone_spacing
            for offset in range(-500, 501):
                incident_zones.add(center + offset)

        while written < target_bytes:
            gap_ms = random.choices(
                [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 5000],
                [10, 15, 20, 20, 15, 8, 5, 3, 2, 1, 1],
            )[0]
            t += datetime.timedelta(milliseconds=gap_ms)

            if i in incident_zones:
                level = random.choices(LEVELS, [1, 5, 20, 30, 44])[0]
            else:
                level = random.choices(LEVELS, LEVEL_WEIGHTS)[0]

            service = random.choice(SERVICES)
            msg = get_msg(level)
            ts_str = t.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]

            line = f"{ts_str} [{level:<5}] [{service}] {msg}\n"
            f.write(line)
            written += len(line)
            line_count += 1
            counts[level] += 1
            i += 1

            if level == "ERROR" and random.random() < 0.4:
                trace_type = random.choices(["java", "rust", "python"], [60, 15, 25])[0]
                if trace_type == "java":
                    trace = random.choice(JAVA_STACK_TRACES)
                elif trace_type == "rust":
                    trace = random.choice(RUST_PANICS)
                else:
                    trace = random.choice(PYTHON_TRACEBACKS)
                for trace_line in trace:
                    line = trace_line + "\n"
                    f.write(line)
                    written += len(line)
                    line_count += 1
                    i += 1

            if level in ("DEBUG", "INFO") and random.random() < 0.02:
                req_id = random.choice(REQUEST_IDS)
                json_lines = [
                    f"  Request details: {{",
                    f'    "request_id": "{req_id}",',
                    f'    "method": "{random.choice(["GET", "POST", "PUT", "DELETE"])}",',
                    f'    "path": "{random.choice(ENDPOINTS)}",',
                    f'    "user_agent": "{random.choice(AGENTS)}",',
                    f'    "remote_addr": "{random.choice(IPS)}"',
                    f"  }}",
                ]
                for jl in json_lines:
                    line = jl + "\n"
                    f.write(line)
                    written += len(line)
                    line_count += 1
                    i += 1

            if i % 500 == 0:
                t += datetime.timedelta(seconds=30)
                ts_str = t.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]
                line = f"{ts_str} [INFO ] [load-balancer] Health check passed: all 8 dependencies healthy\n"
                f.write(line)
                written += len(line)
                line_count += 1
                counts["INFO"] += 1
                i += 1

            if i % 2000 == 0:
                t += datetime.timedelta(milliseconds=100)
                ts_str = t.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]
                job = random.choice(JOBS)
                dur = random.randint(50, 15000)
                line = f"{ts_str} [INFO ] [scheduler] Scheduled job '{job}' completed in {dur}ms\n"
                f.write(line)
                written += len(line)
                line_count += 1
                counts["INFO"] += 1
                i += 1

            if line_count % 1000000 == 0:
                pct = written / target_bytes * 100
                print(f"  {written / (1024*1024*1024):.2f}GB ({pct:.0f}%) - {line_count:,} lines")

        # Shutdown sequence
        t += datetime.timedelta(seconds=1)
        shutdown = [
            (t, "INFO", "api-gateway", "Received SIGTERM, initiating graceful shutdown"),
            (t + datetime.timedelta(milliseconds=100), "INFO", "api-gateway", "Stopping HTTP listener, draining active connections..."),
            (t + datetime.timedelta(milliseconds=500), "INFO", "scheduler", "Cancelling 3 pending scheduled jobs"),
            (t + datetime.timedelta(seconds=1), "INFO", "message-queue", "Closing queue connections, 0 messages pending"),
            (t + datetime.timedelta(seconds=1, milliseconds=200), "INFO", "cache-manager", "Redis connection closed"),
            (t + datetime.timedelta(seconds=1, milliseconds=500), "INFO", "db-pool", "Connection pool drained: 20 connections closed"),
            (t + datetime.timedelta(seconds=2), "INFO", "api-gateway", "Graceful shutdown completed in 2.0s"),
        ]
        for ts, level, service, msg in shutdown:
            line = f"{ts.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]} [{level:<5}] [{service}] {msg}\n"
            f.write(line)
            written += len(line)
            line_count += 1

    print(f"Generated {line_count:,} lines ({written / (1024*1024*1024):.2f}GB) → {output}")
    for l in LEVELS:
        print(f"  {l}: {counts[l]:,}")

if __name__ == "__main__":
    main()
