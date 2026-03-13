# CAD OCR 项目 Dockerfile
# 多阶段构建，优化镜像大小

# ===== 构建阶段 =====
FROM rust:1.75-slim-bookworm AS builder

# 安装依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 设置工作目录
WORKDIR /app

# 复制 Cargo 文件（用于缓存依赖）
COPY Cargo.toml Cargo.lock ./

# 创建空的源文件以缓存依赖
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release
RUN rm -rf src

# 复制源代码
COPY . .

# 构建项目
RUN cargo build --release --bin cad_ocr

# ===== 运行阶段 =====
FROM debian:bookworm-slim AS runtime

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN useradd -m -u 1000 app

# 设置工作目录
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/cad_ocr /app/cad_ocr

# 复制配置文件示例
COPY config.toml.example /app/config.toml

# 设置所有权
RUN chown -R app:app /app

# 切换到非 root 用户
USER app

# 暴露端口
EXPOSE 8080

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/api/v1/health || exit 1

# 启动命令
ENTRYPOINT ["/app/cad_ocr"]
CMD ["--server", "-p", "8080"]
