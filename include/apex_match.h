/**
 * @file apex_match.h
 * @brief C API for apex-match: Ultra-Low-Latency HFT Matching Engine
 * @author koushik4475 <koushik4475@gmail.com>
 * @license MIT OR Apache-2.0
 */

#ifndef APEX_MATCH_H
#define APEX_MATCH_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef uint64_t apex_order_id_t;
typedef uint64_t apex_price_t;     /* Fixed point scaled price (e.g. 10000 = $1.0000) */
typedef uint64_t apex_qty_t;       /* Quantity in lots or base units */
typedef uint32_t apex_trader_id_t;
typedef uint32_t apex_symbol_id_t;

typedef enum {
    APEX_SIDE_BUY  = 0,
    APEX_SIDE_SELL = 1
} apex_side_t;

typedef enum {
    APEX_ORDER_LIMIT       = 0,
    APEX_ORDER_MARKET      = 1,
    APEX_ORDER_POST_ONLY   = 2,
    APEX_ORDER_IOC         = 3, /* Immediate Or Cancel */
    APEX_ORDER_FOK         = 4  /* Fill Or Kill */
} apex_order_type_t;

typedef enum {
    APEX_TIF_GTC = 0, /* Good 'Til Cancelled */
    APEX_TIF_IOC = 1, /* Immediate Or Cancel */
    APEX_TIF_FOK = 2, /* Fill Or Kill */
    APEX_TIF_DAY = 3  /* Good For Day */
} apex_tif_t;

typedef enum {
    APEX_STP_NONE                 = 0,
    APEX_STP_CANCEL_OLDEST        = 1,
    APEX_STP_CANCEL_NEWEST        = 2,
    APEX_STP_DECREMENT_AND_CANCEL = 3
} apex_stp_mode_t;

typedef enum {
    APEX_STATUS_ACCEPTED        = 0,
    APEX_STATUS_FILLED          = 1,
    APEX_STATUS_PARTIALLY_FILLED= 2,
    APEX_STATUS_CANCELLED       = 3,
    APEX_STATUS_REJECTED        = 4,
    APEX_STATUS_EXPIRED         = 5
} apex_exec_status_t;

typedef struct {
    apex_order_id_t   order_id;
    apex_trader_id_t  trader_id;
    apex_symbol_id_t  symbol_id;
    apex_side_t       side;
    apex_order_type_t order_type;
    apex_tif_t        tif;
    apex_stp_mode_t   stp_mode;
    apex_price_t      price;
    apex_qty_t        qty;
    uint64_t          timestamp_ns;
} apex_order_t;

typedef struct {
    uint64_t          match_id;
    apex_order_id_t   maker_order_id;
    apex_order_id_t   taker_order_id;
    apex_trader_id_t  maker_trader_id;
    apex_trader_id_t  taker_trader_id;
    apex_side_t       maker_side;
    apex_price_t      price;
    apex_qty_t        qty;
    uint64_t          timestamp_ns;
} apex_fill_t;

typedef struct {
    apex_order_id_t    order_id;
    apex_trader_id_t   trader_id;
    apex_exec_status_t status;
    apex_qty_t         filled_qty;
    apex_qty_t         leaves_qty;
    apex_price_t       avg_price;
    uint32_t           error_code;
    uint64_t           timestamp_ns;
} apex_execution_report_t;

typedef struct {
    apex_price_t price;
    apex_qty_t   qty;
    uint32_t     order_count;
} apex_l2_level_t;

typedef struct {
    apex_symbol_id_t symbol_id;
    uint64_t         sequence;
    uint64_t         timestamp_ns;
    uint32_t         bid_count;
    uint32_t         ask_count;
    apex_l2_level_t  bids[50];
    apex_l2_level_t  asks[50];
} apex_l2_snapshot_t;

/* Opaque pointer types */
typedef struct apex_engine_s apex_engine_t;

/* Matching Engine C Interface */
apex_engine_t* apex_engine_create(size_t ring_buffer_capacity);
void           apex_engine_destroy(apex_engine_t* engine);

int32_t apex_engine_submit_order(apex_engine_t* engine, const apex_order_t* order);
int32_t apex_engine_cancel_order(apex_engine_t* engine, apex_symbol_id_t symbol_id, apex_order_id_t order_id);
int32_t apex_engine_poll(apex_engine_t* engine, size_t max_events);

int32_t apex_engine_get_l2_snapshot(apex_engine_t* engine, apex_symbol_id_t symbol_id, apex_l2_snapshot_t* out_snapshot);

#ifdef __cplusplus
}
#endif

#endif /* APEX_MATCH_H */
