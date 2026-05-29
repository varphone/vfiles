# VFiles Performance Optimization Guide

This document outlines performance optimizations and best practices for the VFiles application.

## Current Performance Characteristics

### Strengths
- **SQLite WAL mode**: Provides good concurrent read performance
- **Content-addressable storage**: Efficient for large files and deduplication
- **Chunked uploads**: Allows resumable uploads and memory-efficient processing
- **Async/Await**: Non-blocking I/O operations throughout

### Areas for Optimization

## 1. Database Optimizations

### Connection Pool Tuning
```rust
// Current: Basic pool configuration
let pool = SqlitePoolOptions::new()
    .max_connections(10)
    .connect(&database_url)
    .await?;

// Optimized: Production-ready configuration
let pool = SqlitePoolOptions::new()
    .max_connections(20)           // Increase for higher concurrency
    .min_connections(5)            // Keep some connections warm
    .max_lifetime(Duration::hours(1))  // Recycle connections
    .idle_timeout(Duration::minutes(10)) // Close idle connections
    .connect(&database_url)
    .await?;
```

### Query Optimizations

#### Add Database Indexes
Current indexes are minimal. Consider adding:
```sql
-- For file browsing performance
CREATE INDEX idx_entries_namespace_path ON entries(namespace_id, path);
CREATE INDEX idx_entries_parent ON entries(namespace_id, path) WHERE kind = 'directory';

-- For history queries
CREATE INDEX idx_entry_versions_entry_id_version ON entry_versions(entry_id, version DESC);

-- For upload sessions
CREATE INDEX idx_upload_sessions_created_at ON upload_sessions(created_at);
CREATE INDEX idx_upload_parts_session_id ON upload_parts(upload_session_id, part_number);
```

#### Batch Operations
```rust
// Instead of individual inserts, use transactions
let mut tx = pool.begin().await?;
for item in batch {
    sqlx::query("INSERT INTO ...").bind(...).execute(&mut tx).await?;
}
tx.commit().await?;
```

## 2. Memory Optimizations

### Reduce Cloning
Current code has some unnecessary clones. Optimize by:

#### Use References Where Possible
```rust
// Current: Cloning strings
let path = req.path.clone();
let filename = req.filename.clone();

// Optimized: Use references
let path = &req.path;
let filename = &req.filename;
```

#### Arc for Shared State
Current code correctly uses `Arc` for shared repositories, which is good.

### Streaming for Large Files
Current upload implementation loads entire chunks into memory. For very large files:

```rust
// Current: Loads entire chunk into memory
let buf = await slice.arrayBuffer();

// Optimized: Stream processing
let stream = slice.stream();
let reader = stream.getReader();
while (true) {
    let { done, value } = await reader.read();
    if (done) break;
    // Process chunk incrementally
    await processChunk(value);
}
```

## 3. Caching Optimizations

### Response Caching
```rust
// Add cache headers for static content
(
    StatusCode::OK,
    [
        (header::CACHE_CONTROL, "public, max-age=3600"),
        (header::ETAG, etag_value),
    ],
    content
)
```

### Database Query Caching
For frequently accessed data like user permissions:
```rust
// Implement Redis or in-memory cache for session data
// Cache namespace information
// Cache file metadata
```

## 4. Network Optimizations

### Compression
Current nginx config includes gzip compression, which is good.

### Connection Reuse
Current HTTP client configuration should reuse connections.

### Chunked Transfer Encoding
Current implementation uses chunked uploads, which is optimal.

## 5. Security Hardening

### Input Validation
Current validation is basic. Enhance with:

#### File Type Validation
```rust
// More comprehensive MIME type checking
let allowed_mimes = ["image/", "text/", "application/pdf"];
if !allowed_mimes.iter().any(|prefix| mime.starts_with(prefix)) {
    return Err(ValidationError::InvalidFileType);
}
```

#### Path Traversal Protection
Current implementation has basic protection. Enhance with:
```rust
// Additional checks
if path.contains("..") || path.contains("//") || path.starts_with('/') {
    return Err(ValidationError::InvalidPath);
}
// Canonicalize paths
let canonical = std::fs::canonicalize(path)?;
```

### Rate Limiting
Current implementation has basic rate limiting. Consider:
- Per-user rate limits
- Burst allowance
- Progressive delays

### Authentication Security
- Implement proper session invalidation
- Add CSRF protection
- Use secure cookies (HttpOnly, Secure, SameSite)

## 6. Monitoring and Observability

### Metrics Collection
```rust
// Add metrics for:
// - Request latency
// - Database query performance
// - File upload/download rates
// - Error rates by endpoint
// - Memory usage
// - Disk I/O
```

### Structured Logging
Current tracing implementation is good. Consider adding:
- Request IDs for tracing
- Performance timing
- Business metrics

## 7. Scalability Considerations

### Horizontal Scaling
For high-traffic deployments:
- Move to PostgreSQL
- Implement Redis for caching
- Use load balancer
- Consider CDN for static assets

### Database Sharding
For very large deployments:
- Shard by namespace
- Implement database federation
- Use read replicas

## 8. Resource Limits

### File Size Limits
Current: 50MB per file, 100MB total upload
Consider: Configurable limits based on user tiers

### Concurrent Connections
Current: SQLite default limits
Optimize: Connection pool tuning, query optimization

### Memory Usage
Monitor and limit:
- Upload buffer sizes
- Database connection pools
- Cache sizes

## Implementation Priority

### High Priority (Immediate)
1. Add database indexes for common queries
2. Implement proper error handling (avoid unwrap())
3. Add comprehensive input validation
4. Tune database connection pool

### Medium Priority (Next Sprint)
1. Implement response caching
2. Add performance metrics
3. Optimize memory usage in upload handling
4. Add rate limiting

### Low Priority (Future)
1. Implement Redis caching
2. Add horizontal scaling support
3. Implement advanced security features
4. Add performance profiling tools

## Performance Benchmarks

### Target Performance
- API response time: <100ms for simple requests
- File upload: 10MB/s sustained throughput
- Concurrent users: 1000+ simultaneous connections
- Database queries: <10ms average response time

### Monitoring Commands
```bash
# Database performance
sqlite3 data/vfiles.db ".timer on" "SELECT COUNT(*) FROM entries;"

# Memory usage
docker stats vfiles

# Network performance
curl -w "@curl-format.txt" -o /dev/null -s http://localhost:3000/api/health
```

## Security Checklist

- [x] Input validation on all endpoints
- [x] SQL injection prevention (parameterized queries)
- [x] Path traversal protection
- [x] XSS prevention (proper content escaping)
- [x] CSRF protection
- [ ] Rate limiting implementation
- [ ] Secure headers (CSP, HSTS, etc.)
- [ ] Dependency vulnerability scanning
- [ ] Regular security audits

## Next Steps

1. Implement database indexes
2. Add comprehensive error handling
3. Set up performance monitoring
4. Conduct security audit
5. Load testing and optimization