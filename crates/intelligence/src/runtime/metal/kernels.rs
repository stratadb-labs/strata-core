//! Metal Shading Language (MSL) kernel sources.
//!
//! All compute kernels are compiled at runtime from this single source string.
//! MSL is a C++14-based language; we use `metal_stdlib` for math intrinsics.

/// Complete MSL source containing all kernels needed by the MiniLM inference
/// pipeline: GEMM, activations, normalization, attention, and pooling.
pub const MSL_SOURCE: &str = r#"
#include <metal_stdlib>
#include <metal_simdgroup_matrix>
using namespace metal;

// -----------------------------------------------------------------------
// Constants for simdgroup_matrix GEMM tiling
// -----------------------------------------------------------------------
constant constexpr uint BM = 32;
constant constexpr uint BN = 32;
constant constexpr uint BK = 32;

// -----------------------------------------------------------------------
// gemm — simdgroup_matrix 32x32 tiled matrix multiply
//   C[M,N] = A[M,K] * B[K,N]     (both row-major)
//   128 threads (4 simdgroups) per threadgroup, each owns a 16x16 sub-tile.
// -----------------------------------------------------------------------
kernel void gemm(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  M       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    constant     uint&  N       [[buffer(5)]],
    uint2 tgid [[threadgroup_position_in_grid]],
    uint  sgid [[simdgroup_index_in_threadgroup]],
    uint  lid  [[thread_index_in_threadgroup]])
{
    uint tile_row = tgid.y * BM;
    uint tile_col = tgid.x * BN;

    // 2x2 simdgroup layout within the 32x32 tile
    uint sg_row = (sgid / 2) * 16;
    uint sg_col = (sgid % 2) * 16;

    // 2x2 grid of 8x8 accumulators per simdgroup = 16x16 sub-tile
    simdgroup_matrix<float, 8, 8> acc[2][2];
    for (uint i = 0; i < 2; ++i)
        for (uint j = 0; j < 2; ++j)
            acc[i][j] = simdgroup_matrix<float, 8, 8>(0);

    threadgroup float tgA[BM * BK];  // 32x32
    threadgroup float tgB[BK * BN];  // 32x32

    uint num_k_tiles = (K + BK - 1) / BK;

    for (uint kt = 0; kt < num_k_tiles; ++kt) {
        uint k_base = kt * BK;

        // Cooperative load: 128 threads load 32x32 = 1024 elements (8 each)
        for (uint idx = lid; idx < BM * BK; idx += 128) {
            uint r = idx / BK;
            uint c = idx % BK;
            uint gr = tile_row + r;
            uint gc = k_base + c;
            tgA[r * BK + c] = (gr < M && gc < K) ? A[gr * K + gc] : 0.0f;
        }
        for (uint idx = lid; idx < BK * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = k_base + r;
            uint gc = tile_col + c;
            tgB[r * BN + c] = (gr < K && gc < N) ? B[gr * N + gc] : 0.0f;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        // Inner K-loop: 4 steps of 8 along BK=32
        for (uint kk = 0; kk < BK; kk += 8) {
            simdgroup_matrix<float, 8, 8> a_mat[2];
            simdgroup_matrix<float, 8, 8> b_mat[2];

            // Load 2 A sub-matrices (rows sg_row..sg_row+15, cols kk..kk+7)
            simdgroup_load(a_mat[0], &tgA[(sg_row + 0) * BK + kk], BK);
            simdgroup_load(a_mat[1], &tgA[(sg_row + 8) * BK + kk], BK);

            // Load 2 B sub-matrices (rows kk..kk+7, cols sg_col..sg_col+15)
            simdgroup_load(b_mat[0], &tgB[kk * BN + (sg_col + 0)], BN);
            simdgroup_load(b_mat[1], &tgB[kk * BN + (sg_col + 8)], BN);

            // 2x2 multiply-accumulate
            simdgroup_multiply_accumulate(acc[0][0], a_mat[0], b_mat[0], acc[0][0]);
            simdgroup_multiply_accumulate(acc[0][1], a_mat[0], b_mat[1], acc[0][1]);
            simdgroup_multiply_accumulate(acc[1][0], a_mat[1], b_mat[0], acc[1][0]);
            simdgroup_multiply_accumulate(acc[1][1], a_mat[1], b_mat[1], acc[1][1]);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // Store results — threadgroup-level decision to avoid divergent barriers
    uint out_row = tile_row + sg_row;
    uint out_col = tile_col + sg_col;

    // Check if the entire 32x32 threadgroup tile is in-bounds
    if (tile_row + BM <= M && tile_col + BN <= N) {
        // Fast path: all simdgroups store directly to device memory
        simdgroup_store(acc[0][0], &C[(out_row + 0) * N + (out_col + 0)], N);
        simdgroup_store(acc[0][1], &C[(out_row + 0) * N + (out_col + 8)], N);
        simdgroup_store(acc[1][0], &C[(out_row + 8) * N + (out_col + 0)], N);
        simdgroup_store(acc[1][1], &C[(out_row + 8) * N + (out_col + 8)], N);
    } else {
        // Edge tile: all simdgroups store to staging, then bounds-checked write
        threadgroup float staging[BM * BN];
        uint base = sg_row * BN + sg_col;
        simdgroup_store(acc[0][0], &staging[base + 0 * BN + 0], BN);
        simdgroup_store(acc[0][1], &staging[base + 0 * BN + 8], BN);
        simdgroup_store(acc[1][0], &staging[base + 8 * BN + 0], BN);
        simdgroup_store(acc[1][1], &staging[base + 8 * BN + 8], BN);

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint idx = lid; idx < BM * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = tile_row + r;
            uint gc = tile_col + c;
            if (gr < M && gc < N) {
                C[gr * N + gc] = staging[r * BN + c];
            }
        }
    }
}

// -----------------------------------------------------------------------
// gemm_transpose — simdgroup_matrix GEMM where B is (N,K) accessed transposed
//   C[M,N] = A[M,K] * B^T   where B is stored as (N,K)
// -----------------------------------------------------------------------
kernel void gemm_transpose(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  M       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    constant     uint&  N       [[buffer(5)]],
    uint2 tgid [[threadgroup_position_in_grid]],
    uint  sgid [[simdgroup_index_in_threadgroup]],
    uint  lid  [[thread_index_in_threadgroup]])
{
    uint tile_row = tgid.y * BM;
    uint tile_col = tgid.x * BN;

    uint sg_row = (sgid / 2) * 16;
    uint sg_col = (sgid % 2) * 16;

    simdgroup_matrix<float, 8, 8> acc[2][2];
    for (uint i = 0; i < 2; ++i)
        for (uint j = 0; j < 2; ++j)
            acc[i][j] = simdgroup_matrix<float, 8, 8>(0);

    threadgroup float tgA[BM * BK];
    threadgroup float tgB[BK * BN];

    uint num_k_tiles = (K + BK - 1) / BK;

    for (uint kt = 0; kt < num_k_tiles; ++kt) {
        uint k_base = kt * BK;

        // Load A tile (same as gemm)
        for (uint idx = lid; idx < BM * BK; idx += 128) {
            uint r = idx / BK;
            uint c = idx % BK;
            uint gr = tile_row + r;
            uint gc = k_base + c;
            tgA[r * BK + c] = (gr < M && gc < K) ? A[gr * K + gc] : 0.0f;
        }
        // Load B tile transposed: B is (N,K), we want B^T[k,n] = B[n,k]
        // tgB[r][c] corresponds to K-dim r, N-dim c
        for (uint idx = lid; idx < BK * BN; idx += 128) {
            uint r = idx / BN;  // K-dim offset
            uint c = idx % BN;  // N-dim offset
            uint gk = k_base + r;
            uint gn = tile_col + c;
            tgB[r * BN + c] = (gk < K && gn < N) ? B[gn * K + gk] : 0.0f;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint kk = 0; kk < BK; kk += 8) {
            simdgroup_matrix<float, 8, 8> a_mat[2];
            simdgroup_matrix<float, 8, 8> b_mat[2];

            simdgroup_load(a_mat[0], &tgA[(sg_row + 0) * BK + kk], BK);
            simdgroup_load(a_mat[1], &tgA[(sg_row + 8) * BK + kk], BK);

            simdgroup_load(b_mat[0], &tgB[kk * BN + (sg_col + 0)], BN);
            simdgroup_load(b_mat[1], &tgB[kk * BN + (sg_col + 8)], BN);

            simdgroup_multiply_accumulate(acc[0][0], a_mat[0], b_mat[0], acc[0][0]);
            simdgroup_multiply_accumulate(acc[0][1], a_mat[0], b_mat[1], acc[0][1]);
            simdgroup_multiply_accumulate(acc[1][0], a_mat[1], b_mat[0], acc[1][0]);
            simdgroup_multiply_accumulate(acc[1][1], a_mat[1], b_mat[1], acc[1][1]);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    uint out_row = tile_row + sg_row;
    uint out_col = tile_col + sg_col;

    if (tile_row + BM <= M && tile_col + BN <= N) {
        simdgroup_store(acc[0][0], &C[(out_row + 0) * N + (out_col + 0)], N);
        simdgroup_store(acc[0][1], &C[(out_row + 0) * N + (out_col + 8)], N);
        simdgroup_store(acc[1][0], &C[(out_row + 8) * N + (out_col + 0)], N);
        simdgroup_store(acc[1][1], &C[(out_row + 8) * N + (out_col + 8)], N);
    } else {
        threadgroup float staging[BM * BN];
        uint base = sg_row * BN + sg_col;
        simdgroup_store(acc[0][0], &staging[base + 0 * BN + 0], BN);
        simdgroup_store(acc[0][1], &staging[base + 0 * BN + 8], BN);
        simdgroup_store(acc[1][0], &staging[base + 8 * BN + 0], BN);
        simdgroup_store(acc[1][1], &staging[base + 8 * BN + 8], BN);

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint idx = lid; idx < BM * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = tile_row + r;
            uint gc = tile_col + c;
            if (gr < M && gc < N) {
                C[gr * N + gc] = staging[r * BN + c];
            }
        }
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
    // Clamp before tanh to avoid NaN from fast-math exp overflow
    inner = clamp(inner, -10.0f, 10.0f);
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

// -----------------------------------------------------------------------
// batched_gemm_transpose — block-diagonal C[b] = A[b] * B[b]^T
//   A: (batch*S, K), B: (batch*S, K), C: (batch*S, S)
//   Each batch: (S,K) x (S,K)^T -> (S,S)
//   simdgroup_matrix with batch index from threadgroup z.
// -----------------------------------------------------------------------
kernel void batched_gemm_transpose(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  S       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint  sgid [[simdgroup_index_in_threadgroup]],
    uint  lid  [[thread_index_in_threadgroup]])
{
    uint batch = tgid.z;
    uint tile_row = tgid.y * BM;
    uint tile_col = tgid.x * BN;

    uint a_off = batch * S * K;
    uint b_off = batch * S * K;
    uint c_off = batch * S * S;

    uint sg_row = (sgid / 2) * 16;
    uint sg_col = (sgid % 2) * 16;

    simdgroup_matrix<float, 8, 8> acc[2][2];
    for (uint i = 0; i < 2; ++i)
        for (uint j = 0; j < 2; ++j)
            acc[i][j] = simdgroup_matrix<float, 8, 8>(0);

    threadgroup float tgA[BM * BK];
    threadgroup float tgB[BK * BN];

    uint num_k_tiles = (K + BK - 1) / BK;

    for (uint kt = 0; kt < num_k_tiles; ++kt) {
        uint k_base = kt * BK;

        for (uint idx = lid; idx < BM * BK; idx += 128) {
            uint r = idx / BK;
            uint c = idx % BK;
            uint gr = tile_row + r;
            uint gc = k_base + c;
            tgA[r * BK + c] = (gr < S && gc < K) ? A[a_off + gr * K + gc] : 0.0f;
        }
        // B transposed: B is (S,K), we want B^T[k,n] = B[n,k]
        for (uint idx = lid; idx < BK * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gk = k_base + r;
            uint gn = tile_col + c;
            tgB[r * BN + c] = (gk < K && gn < S) ? B[b_off + gn * K + gk] : 0.0f;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint kk = 0; kk < BK; kk += 8) {
            simdgroup_matrix<float, 8, 8> a_mat[2];
            simdgroup_matrix<float, 8, 8> b_mat[2];

            simdgroup_load(a_mat[0], &tgA[(sg_row + 0) * BK + kk], BK);
            simdgroup_load(a_mat[1], &tgA[(sg_row + 8) * BK + kk], BK);

            simdgroup_load(b_mat[0], &tgB[kk * BN + (sg_col + 0)], BN);
            simdgroup_load(b_mat[1], &tgB[kk * BN + (sg_col + 8)], BN);

            simdgroup_multiply_accumulate(acc[0][0], a_mat[0], b_mat[0], acc[0][0]);
            simdgroup_multiply_accumulate(acc[0][1], a_mat[0], b_mat[1], acc[0][1]);
            simdgroup_multiply_accumulate(acc[1][0], a_mat[1], b_mat[0], acc[1][0]);
            simdgroup_multiply_accumulate(acc[1][1], a_mat[1], b_mat[1], acc[1][1]);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    uint out_row = tile_row + sg_row;
    uint out_col = tile_col + sg_col;

    if (tile_row + BM <= S && tile_col + BN <= S) {
        simdgroup_store(acc[0][0], &C[c_off + (out_row + 0) * S + (out_col + 0)], S);
        simdgroup_store(acc[0][1], &C[c_off + (out_row + 0) * S + (out_col + 8)], S);
        simdgroup_store(acc[1][0], &C[c_off + (out_row + 8) * S + (out_col + 0)], S);
        simdgroup_store(acc[1][1], &C[c_off + (out_row + 8) * S + (out_col + 8)], S);
    } else {
        threadgroup float staging[BM * BN];
        uint base = sg_row * BN + sg_col;
        simdgroup_store(acc[0][0], &staging[base + 0 * BN + 0], BN);
        simdgroup_store(acc[0][1], &staging[base + 0 * BN + 8], BN);
        simdgroup_store(acc[1][0], &staging[base + 8 * BN + 0], BN);
        simdgroup_store(acc[1][1], &staging[base + 8 * BN + 8], BN);

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint idx = lid; idx < BM * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = tile_row + r;
            uint gc = tile_col + c;
            if (gr < S && gc < S) {
                C[c_off + gr * S + gc] = staging[r * BN + c];
            }
        }
    }
}

// -----------------------------------------------------------------------
// batched_gemm — block-diagonal C[b] = A[b] * B[b]
//   A: (batch*S, S), B: (batch*S, K), C: (batch*S, K)
//   Each batch: (S,S) x (S,K) -> (S,K)
//   simdgroup_matrix with batch index from threadgroup z.
// -----------------------------------------------------------------------
kernel void batched_gemm(
    device const float* A       [[buffer(0)]],
    device const float* B       [[buffer(1)]],
    device       float* C       [[buffer(2)]],
    constant     uint&  S       [[buffer(3)]],
    constant     uint&  K       [[buffer(4)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint  sgid [[simdgroup_index_in_threadgroup]],
    uint  lid  [[thread_index_in_threadgroup]])
{
    uint batch = tgid.z;
    uint tile_row = tgid.y * BM;
    uint tile_col = tgid.x * BN;

    uint a_off = batch * S * S;
    uint b_off = batch * S * K;
    uint c_off = batch * S * K;

    uint sg_row = (sgid / 2) * 16;
    uint sg_col = (sgid % 2) * 16;

    simdgroup_matrix<float, 8, 8> acc[2][2];
    for (uint i = 0; i < 2; ++i)
        for (uint j = 0; j < 2; ++j)
            acc[i][j] = simdgroup_matrix<float, 8, 8>(0);

    threadgroup float tgA[BM * BK];
    threadgroup float tgB[BK * BN];

    // Inner dimension for batched_gemm is S (A is SxS)
    uint num_k_tiles = (S + BK - 1) / BK;

    for (uint kt = 0; kt < num_k_tiles; ++kt) {
        uint k_base = kt * BK;

        // Load A tile (S x S)
        for (uint idx = lid; idx < BM * BK; idx += 128) {
            uint r = idx / BK;
            uint c = idx % BK;
            uint gr = tile_row + r;
            uint gc = k_base + c;
            tgA[r * BK + c] = (gr < S && gc < S) ? A[a_off + gr * S + gc] : 0.0f;
        }
        // Load B tile (S x K)
        for (uint idx = lid; idx < BK * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = k_base + r;
            uint gc = tile_col + c;
            tgB[r * BN + c] = (gr < S && gc < K) ? B[b_off + gr * K + gc] : 0.0f;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint kk = 0; kk < BK; kk += 8) {
            simdgroup_matrix<float, 8, 8> a_mat[2];
            simdgroup_matrix<float, 8, 8> b_mat[2];

            simdgroup_load(a_mat[0], &tgA[(sg_row + 0) * BK + kk], BK);
            simdgroup_load(a_mat[1], &tgA[(sg_row + 8) * BK + kk], BK);

            simdgroup_load(b_mat[0], &tgB[kk * BN + (sg_col + 0)], BN);
            simdgroup_load(b_mat[1], &tgB[kk * BN + (sg_col + 8)], BN);

            simdgroup_multiply_accumulate(acc[0][0], a_mat[0], b_mat[0], acc[0][0]);
            simdgroup_multiply_accumulate(acc[0][1], a_mat[0], b_mat[1], acc[0][1]);
            simdgroup_multiply_accumulate(acc[1][0], a_mat[1], b_mat[0], acc[1][0]);
            simdgroup_multiply_accumulate(acc[1][1], a_mat[1], b_mat[1], acc[1][1]);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    uint out_row = tile_row + sg_row;
    uint out_col = tile_col + sg_col;

    if (tile_row + BM <= S && tile_col + BN <= K) {
        simdgroup_store(acc[0][0], &C[c_off + (out_row + 0) * K + (out_col + 0)], K);
        simdgroup_store(acc[0][1], &C[c_off + (out_row + 0) * K + (out_col + 8)], K);
        simdgroup_store(acc[1][0], &C[c_off + (out_row + 8) * K + (out_col + 0)], K);
        simdgroup_store(acc[1][1], &C[c_off + (out_row + 8) * K + (out_col + 8)], K);
    } else {
        threadgroup float staging[BM * BN];
        uint base = sg_row * BN + sg_col;
        simdgroup_store(acc[0][0], &staging[base + 0 * BN + 0], BN);
        simdgroup_store(acc[0][1], &staging[base + 0 * BN + 8], BN);
        simdgroup_store(acc[1][0], &staging[base + 8 * BN + 0], BN);
        simdgroup_store(acc[1][1], &staging[base + 8 * BN + 8], BN);

        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint idx = lid; idx < BM * BN; idx += 128) {
            uint r = idx / BN;
            uint c = idx % BN;
            uint gr = tile_row + r;
            uint gc = tile_col + c;
            if (gr < S && gc < K) {
                C[c_off + gr * K + gc] = staging[r * BN + c];
            }
        }
    }
}

// -----------------------------------------------------------------------
// batched_attention_mask — per-sequence masking for batched attention
//   scores: (batch*S, S), mask: (batch*S,)
//   For row r: batch = r / seq_len,
//   if mask[batch * seq_len + col] == 0 then scores[r][col] = -10000
// -----------------------------------------------------------------------
kernel void batched_attention_mask(
    device       float* scores   [[buffer(0)]],
    device const uint*  mask     [[buffer(1)]],
    constant     uint&  total_rows [[buffer(2)]],
    constant     uint&  seq_len  [[buffer(3)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint r = gid.y;
    uint c = gid.x;
    if (r >= total_rows || c >= seq_len) return;
    uint batch = r / seq_len;
    if (mask[batch * seq_len + c] == 0u) {
        scores[r * seq_len + c] = -10000.0f;
    }
}

// -----------------------------------------------------------------------
// transpose_heads — rearrange (B*S, H*D) -> (B*H*S, D)
//   Moves head dimension into the batch dimension for batched attention.
//   2D grid: (ceil(D/16), ceil(B*S/16)), with H in z dimension.
// -----------------------------------------------------------------------
kernel void transpose_heads(
    device const float* src       [[buffer(0)]],
    device       float* dst       [[buffer(1)]],
    constant     uint&  batch_size [[buffer(2)]],
    constant     uint&  seq_len   [[buffer(3)]],
    constant     uint&  num_heads [[buffer(4)]],
    constant     uint&  head_dim  [[buffer(5)]],
    uint3 gid [[thread_position_in_grid]])
{
    uint d = gid.x;   // column in head_dim
    uint s = gid.y;   // row in B*S space (flattened batch*seq)
    uint h = gid.z;   // head index

    if (d >= head_dim || s >= batch_size * seq_len) return;

    uint b = s / seq_len;
    uint seq = s % seq_len;

    // src layout: [b*S+seq, h*D+d]  — row (b*S+seq), col (h*D+d)
    uint src_idx = s * (num_heads * head_dim) + h * head_dim + d;
    // dst layout: [b*H*S + h*S + seq, d]
    uint dst_idx = (b * num_heads * seq_len + h * seq_len + seq) * head_dim + d;

    dst[dst_idx] = src[src_idx];
}

// -----------------------------------------------------------------------
// untranspose_heads — rearrange (B*H*S, D) -> (B*S, H*D)
//   Inverse of transpose_heads.
//   2D grid: (ceil(D/16), ceil(B*S/16)), with H in z dimension.
// -----------------------------------------------------------------------
kernel void untranspose_heads(
    device const float* src       [[buffer(0)]],
    device       float* dst       [[buffer(1)]],
    constant     uint&  batch_size [[buffer(2)]],
    constant     uint&  seq_len   [[buffer(3)]],
    constant     uint&  num_heads [[buffer(4)]],
    constant     uint&  head_dim  [[buffer(5)]],
    uint3 gid [[thread_position_in_grid]])
{
    uint d = gid.x;   // column in head_dim
    uint s = gid.y;   // row in B*S space (flattened batch*seq)
    uint h = gid.z;   // head index

    if (d >= head_dim || s >= batch_size * seq_len) return;

    uint b = s / seq_len;
    uint seq = s % seq_len;

    // src layout: [b*H*S + h*S + seq, d]
    uint src_idx = (b * num_heads * seq_len + h * seq_len + seq) * head_dim + d;
    // dst layout: [b*S+seq, h*D+d]
    uint dst_idx = s * (num_heads * head_dim) + h * head_dim + d;

    dst[dst_idx] = src[src_idx];
}

// -----------------------------------------------------------------------
// multi_head_batched_attention_mask — masking for (B*H*S, S) scores
//   Every H consecutive batches share one mask row.
//   For row r: group = r / (H * S), mask_idx = group * S + col.
//   2D grid: (ceil(S/16), ceil(total_rows/16))
// -----------------------------------------------------------------------
kernel void multi_head_batched_attention_mask(
    device       float* scores     [[buffer(0)]],
    device const uint*  mask       [[buffer(1)]],
    constant     uint&  total_rows [[buffer(2)]],
    constant     uint&  seq_len    [[buffer(3)]],
    constant     uint&  num_heads  [[buffer(4)]],
    uint2 gid [[thread_position_in_grid]])
{
    uint c = gid.x;
    uint r = gid.y;
    if (r >= total_rows || c >= seq_len) return;

    uint group_size = num_heads * seq_len;
    uint group = r / group_size;
    uint mask_idx = group * seq_len + c;

    if (mask[mask_idx] == 0u) {
        scores[r * seq_len + c] = -10000.0f;
    }
}

// -----------------------------------------------------------------------
// batched_mean_pool — per-sequence mean pooling
//   hidden: (batch*S, D), mask: (batch*S,), output: (batch, D)
//   One threadgroup per batch. Each thread handles columns at stride.
// -----------------------------------------------------------------------
kernel void batched_mean_pool(
    device const float* hidden   [[buffer(0)]],
    device const uint*  mask     [[buffer(1)]],
    device       float* output   [[buffer(2)]],
    constant     uint&  seq_len  [[buffer(3)]],
    constant     uint&  cols     [[buffer(4)]],
    uint gid  [[threadgroup_position_in_grid]],
    uint lid  [[thread_position_in_threadgroup]],
    uint threads_per_group [[threads_per_threadgroup]])
{
    uint batch = gid;

    // Zero output columns handled by this thread.
    for (uint c = lid; c < cols; c += threads_per_group) {
        output[batch * cols + c] = 0.0f;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Count masked tokens in this batch.
    float count = 0.0f;
    for (uint s = 0; s < seq_len; ++s) {
        if (mask[batch * seq_len + s] == 1u) {
            count += 1.0f;
        }
    }

    // Accumulate masked rows.
    for (uint s = 0; s < seq_len; ++s) {
        if (mask[batch * seq_len + s] == 1u) {
            uint row_off = (batch * seq_len + s) * cols;
            for (uint c = lid; c < cols; c += threads_per_group) {
                output[batch * cols + c] += hidden[row_off + c];
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Divide by count.
    if (count > 0.0f) {
        float inv_count = 1.0f / count;
        for (uint c = lid; c < cols; c += threads_per_group) {
            output[batch * cols + c] *= inv_count;
        }
    }
}
"#;
