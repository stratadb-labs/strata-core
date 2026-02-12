//! Metal Shading Language (MSL) kernel sources.
//!
//! All compute kernels are compiled at runtime from this single source string.
//! MSL is a C++14-based language; we use `metal_stdlib` for math intrinsics.

/// Complete MSL source containing all kernels needed by the MiniLM inference
/// pipeline: GEMM, activations, normalization, attention, and pooling.
pub const MSL_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

// -----------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------
constant constexpr uint TILE = 16;

// -----------------------------------------------------------------------
// gemm — tiled 16x16 shared-memory matrix multiply
//   C[M,N] = A[M,K] * B[K,N]     (both row-major)
// -----------------------------------------------------------------------
kernel void gemm(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  M       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    constant     uint&  N       [[buffer(5)]],
    uint2 gid  [[thread_position_in_grid]],
    uint2 lid  [[thread_position_in_threadgroup]])
{
    // gid.x = column, gid.y = row
    uint row = gid.y;
    uint col = gid.x;

    threadgroup float tileA[TILE][TILE];
    threadgroup float tileB[TILE][TILE];

    float sum = 0.0f;

    uint numTiles = (K + TILE - 1) / TILE;
    for (uint t = 0; t < numTiles; ++t) {
        // Load tile of A
        uint aCol = t * TILE + lid.x;
        uint aRow = row;
        tileA[lid.y][lid.x] = (aRow < M && aCol < K) ? A[aRow * K + aCol] : 0.0f;

        // Load tile of B
        uint bRow = t * TILE + lid.y;
        uint bCol = col;
        tileB[lid.y][lid.x] = (bRow < K && bCol < N) ? B[bRow * N + bCol] : 0.0f;

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint i = 0; i < TILE; ++i) {
            sum += tileA[lid.y][i] * tileB[i][lid.x];
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (row < M && col < N) {
        C[row * N + col] = sum;
    }
}

// -----------------------------------------------------------------------
// gemm_transpose — tiled GEMM where B is (N,K) accessed transposed
//   C[M,N] = A[M,K] * B^T   where B is stored as (N,K)
// -----------------------------------------------------------------------
kernel void gemm_transpose(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  M       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    constant     uint&  N       [[buffer(5)]],
    uint2 gid  [[thread_position_in_grid]],
    uint2 lid  [[thread_position_in_threadgroup]])
{
    uint row = gid.y;
    uint col = gid.x;

    threadgroup float tileA[TILE][TILE];
    threadgroup float tileB[TILE][TILE];

    float sum = 0.0f;

    uint numTiles = (K + TILE - 1) / TILE;
    for (uint t = 0; t < numTiles; ++t) {
        // Load tile of A  (M x K, row-major)
        uint aCol = t * TILE + lid.x;
        uint aRow = row;
        tileA[lid.y][lid.x] = (aRow < M && aCol < K) ? A[aRow * K + aCol] : 0.0f;

        // Load tile of B^T — B is (N x K), so B^T[k][n] = B[n][k]
        // We want tileB[lid.y][lid.x] = B^T[t*TILE+lid.y][col-base+lid.x]
        // = B[col-base+lid.x][t*TILE+lid.y]
        // But we need the tile indexed as (k-tile-row, n-tile-col).
        // tileB[ky][nx] where ky indexes into the K dimension, nx into N.
        uint bK   = t * TILE + lid.y;       // K-dimension index
        uint bN   = col;                     // N-dimension index (same threadgroup column)
        // B is stored row-major as (N, K), so element (n, k) = B[n * K + k]
        // We want B^T element (k, n) = B[n * K + k]
        // But here we need to load per-threadgroup tile. Each thread loads one element.
        // Remap: we load B[col_base + lid.x][ t*TILE + lid.y ]
        uint bN2 = (gid.x - lid.x) + lid.x; // = gid.x = col ... same as col
        tileB[lid.y][lid.x] = (bK < K && bN2 < N) ? B[bN2 * K + bK] : 0.0f;

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint i = 0; i < TILE; ++i) {
            sum += tileA[lid.y][i] * tileB[i][lid.x];
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (row < M && col < N) {
        C[row * N + col] = sum;
    }
}

// -----------------------------------------------------------------------
// gelu — element-wise fast GELU approximation
//   y = x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
// -----------------------------------------------------------------------
kernel void gelu(
    device const float* input  [[buffer(0)]],
    device       float* output [[buffer(1)]],
    constant     uint&  count  [[buffer(2)]],
    uint tid [[thread_position_in_grid]])
{
    if (tid >= count) return;
    float x = input[tid];
    const float SQRT_2_OVER_PI = 0.7978845608f;
    float inner = SQRT_2_OVER_PI * (x + 0.044715f * x * x * x);
    output[tid] = 0.5f * x * (1.0f + metal::tanh(inner));
}

// -----------------------------------------------------------------------
// add_tensor — element-wise addition  c[i] = a[i] + b[i]
// -----------------------------------------------------------------------
kernel void add_tensor(
    device const float* a      [[buffer(0)]],
    device const float* b      [[buffer(1)]],
    device       float* c      [[buffer(2)]],
    constant     uint&  count  [[buffer(3)]],
    uint tid [[thread_position_in_grid]])
{
    if (tid >= count) return;
    c[tid] = a[tid] + b[tid];
}

// -----------------------------------------------------------------------
// add_bias — broadcast row-add:  t[r*cols+c] += bias[c]
// -----------------------------------------------------------------------
kernel void add_bias(
    device       float* t      [[buffer(0)]],
    device const float* bias   [[buffer(1)]],
    constant     uint&  rows   [[buffer(2)]],
    constant     uint&  cols   [[buffer(3)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint r = gid.y;
    uint c = gid.x;
    if (r >= rows || c >= cols) return;
    t[r * cols + c] += bias[c];
}

// -----------------------------------------------------------------------
// scale_kernel — in-place scalar multiply  t[i] *= factor
// -----------------------------------------------------------------------
kernel void scale_kernel(
    device       float* t      [[buffer(0)]],
    constant     float& factor [[buffer(1)]],
    constant     uint&  count  [[buffer(2)]],
    uint tid [[thread_position_in_grid]])
{
    if (tid >= count) return;
    t[tid] *= factor;
}

// -----------------------------------------------------------------------
// layer_norm — per-row layer normalization
//   out[r,c] = (x[r,c] - mean_r) / sqrt(var_r + eps) * w[c] + b[c]
//   One threadgroup per row; shared-memory reduction for mean and variance.
// -----------------------------------------------------------------------
kernel void layer_norm(
    device const float* input   [[buffer(0)]],
    device const float* weight  [[buffer(1)]],
    device const float* bias    [[buffer(2)]],
    device       float* output  [[buffer(3)]],
    constant     uint&  rows    [[buffer(4)]],
    constant     uint&  cols    [[buffer(5)]],
    constant     float& eps     [[buffer(6)]],
    uint gid  [[threadgroup_position_in_grid]],
    uint lid  [[thread_position_in_threadgroup]],
    uint threads_per_group [[threads_per_threadgroup]])
{
    uint row = gid;
    if (row >= rows) return;

    threadgroup float shared_data[256];

    // --- Compute mean ---
    float partial_sum = 0.0f;
    for (uint c = lid; c < cols; c += threads_per_group) {
        partial_sum += input[row * cols + c];
    }
    shared_data[lid] = partial_sum;

    // Tree reduction
    for (uint stride = threads_per_group / 2; stride > 0; stride >>= 1) {
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (lid < stride) {
            shared_data[lid] += shared_data[lid + stride];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    float mean = shared_data[0] / float(cols);

    // --- Compute variance ---
    float partial_var = 0.0f;
    for (uint c = lid; c < cols; c += threads_per_group) {
        float diff = input[row * cols + c] - mean;
        partial_var += diff * diff;
    }
    shared_data[lid] = partial_var;

    for (uint stride = threads_per_group / 2; stride > 0; stride >>= 1) {
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (lid < stride) {
            shared_data[lid] += shared_data[lid + stride];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    float var = shared_data[0] / float(cols);
    float inv_std = 1.0f / sqrt(var + eps);

    // --- Normalize ---
    for (uint c = lid; c < cols; c += threads_per_group) {
        uint idx = row * cols + c;
        output[idx] = (input[idx] - mean) * inv_std * weight[c] + bias[c];
    }
}

// -----------------------------------------------------------------------
// softmax_rows — per-row softmax with max subtraction for stability
//   One threadgroup per row; shared-memory reductions for max and sum.
// -----------------------------------------------------------------------
kernel void softmax_rows(
    device float* data          [[buffer(0)]],
    constant uint& rows         [[buffer(1)]],
    constant uint& cols         [[buffer(2)]],
    uint gid  [[threadgroup_position_in_grid]],
    uint lid  [[thread_position_in_threadgroup]],
    uint threads_per_group [[threads_per_threadgroup]])
{
    uint row = gid;
    if (row >= rows) return;

    threadgroup float shared[256];

    // --- Find row max ---
    float local_max = -INFINITY;
    for (uint c = lid; c < cols; c += threads_per_group) {
        local_max = max(local_max, data[row * cols + c]);
    }
    shared[lid] = local_max;

    for (uint stride = threads_per_group / 2; stride > 0; stride >>= 1) {
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (lid < stride) {
            shared[lid] = max(shared[lid], shared[lid + stride]);
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    float row_max = shared[0];

    // --- Compute exp(x - max) and partial sum ---
    float local_sum = 0.0f;
    for (uint c = lid; c < cols; c += threads_per_group) {
        uint idx = row * cols + c;
        float val = exp(data[idx] - row_max);
        data[idx] = val;
        local_sum += val;
    }
    shared[lid] = local_sum;

    for (uint stride = threads_per_group / 2; stride > 0; stride >>= 1) {
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (lid < stride) {
            shared[lid] += shared[lid + stride];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    float total = shared[0];

    // --- Normalize ---
    if (total > 0.0f) {
        float inv_total = 1.0f / total;
        for (uint c = lid; c < cols; c += threads_per_group) {
            data[row * cols + c] *= inv_total;
        }
    }
}

// -----------------------------------------------------------------------
// slice_columns — copy a contiguous column range from src to dst
//   dst[r, c] = src[r, start + c]   for c in [0, width)
// -----------------------------------------------------------------------
kernel void slice_columns(
    device const float* src     [[buffer(0)]],
    device       float* dst     [[buffer(1)]],
    constant     uint&  src_cols [[buffer(2)]],
    constant     uint&  start   [[buffer(3)]],
    constant     uint&  width   [[buffer(4)]],
    constant     uint&  rows    [[buffer(5)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint r = gid.y;
    uint c = gid.x;
    if (r >= rows || c >= width) return;
    dst[r * width + c] = src[r * src_cols + start + c];
}

// -----------------------------------------------------------------------
// scatter_columns — write src columns into dst at col_offset
//   dst[r, col_offset + c] = src[r, c]
// -----------------------------------------------------------------------
kernel void scatter_columns(
    device       float* dst       [[buffer(0)]],
    device const float* src       [[buffer(1)]],
    constant     uint&  dst_cols  [[buffer(2)]],
    constant     uint&  src_cols  [[buffer(3)]],
    constant     uint&  col_off   [[buffer(4)]],
    constant     uint&  rows      [[buffer(5)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint r = gid.y;
    uint c = gid.x;
    if (r >= rows || c >= src_cols) return;
    dst[r * dst_cols + col_off + c] = src[r * src_cols + c];
}

// -----------------------------------------------------------------------
// attention_mask — set scores to -10000 where mask is 0
//   scores[i, j] = (mask[j] == 0) ? -10000.0 : scores[i, j]
// -----------------------------------------------------------------------
kernel void attention_mask(
    device       float* scores  [[buffer(0)]],
    device const uint*  mask    [[buffer(1)]],
    constant     uint&  rows    [[buffer(2)]],
    constant     uint&  cols    [[buffer(3)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint r = gid.y;
    uint c = gid.x;
    if (r >= rows || c >= cols) return;
    if (mask[c] == 0u) {
        scores[r * cols + c] = -10000.0f;
    }
}

// -----------------------------------------------------------------------
// mean_pool — sum masked rows, divide by count
//   output[c] = sum_over_r(mask[r] ? hidden[r,c] : 0) / count
//   One threadgroup handles one column.
// -----------------------------------------------------------------------
kernel void mean_pool(
    device const float* hidden  [[buffer(0)]],
    device const uint*  mask    [[buffer(1)]],
    device       float* output  [[buffer(2)]],
    constant     uint&  rows    [[buffer(3)]],
    constant     uint&  cols    [[buffer(4)]],
    uint tid [[thread_position_in_grid]])
{
    if (tid >= cols) return;

    float sum = 0.0f;
    float count = 0.0f;
    for (uint r = 0; r < rows; ++r) {
        if (mask[r] == 1u) {
            sum += hidden[r * cols + tid];
            count += 1.0f;
        }
    }
    output[tid] = (count > 0.0f) ? (sum / count) : 0.0f;
}
"#;
