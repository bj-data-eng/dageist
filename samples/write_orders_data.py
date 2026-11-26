"""Writes the data files of the sample workspace: two Parquet tables and two CSV tables.

Usage: python samples/write_orders_data.py samples/orders/data

Run with the Python Polars of the release the engine is built from. The rows are written out
here by hand, so the files are the same each time they are written.
"""
import sys
from datetime import datetime
from pathlib import Path

import polars as pl

data = Path(sys.argv[1])
data.mkdir(parents=True, exist_ok=True)

pl.DataFrame(
    {
        "order_id": [1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1009, 1010, 1011, 1012],
        "customer_id": [1, 2, 1, 3, 4, 2, 3, 1, 4, 2, 3, 4],
        "product_id": [1, 4, 5, 2, 3, 5, 1, 6, 4, 3, 2, 6],
        "quantity": [2, 10, 1, 4, 3, 25, 1, 2, 12, 1, 6, 3],
        "ordered_at": [
            datetime(2026, 3, 2, 9, 15),
            datetime(2026, 3, 2, 13, 30),
            datetime(2026, 3, 3, 16, 40),
            datetime(2026, 3, 4, 10, 0),
            datetime(2026, 3, 4, 17, 20),
            datetime(2026, 3, 5, 7, 50),
            datetime(2026, 3, 6, 11, 5),
            datetime(2026, 3, 9, 14, 45),
            datetime(2026, 3, 10, 8, 30),
            datetime(2026, 3, 11, 15, 10),
            datetime(2026, 3, 12, 12, 0),
            datetime(2026, 3, 13, 16, 25),
        ],
    },
    schema_overrides={"ordered_at": pl.Datetime("us")},
).write_parquet(data / "orders.parquet")

pl.DataFrame(
    {
        "product_id": [1, 2, 3, 4, 5, 6],
        "product": ["Desk", "Chair", "Lamp", "Notebook", "Pen", "Stapler"],
        "category": ["Furniture", "Furniture", "Lighting", "Stationery", "Stationery", "Stationery"],
        "unit_price": [250.0, 120.0, 45.0, 4.5, 1.2, 15.0],
    }
).write_parquet(data / "products.parquet")

(data / "customers.csv").write_text(
    "customer_id,customer,region\n"
    "1,Ridgeway Traders,North\n"
    "2,Harbor Supply,South\n"
    "3,Summit Office,North\n"
    "4,Lakeside Studio,West\n"
)

(data / "categories.csv").write_text(
    "category,description\n"
    "Furniture,Desks and chairs\n"
    "Lighting,Lamps\n"
    "Stationery,Paper and pens\n"
)
