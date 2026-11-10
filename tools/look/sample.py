"""A smoke test of the kit."""
from lib import box, cyl, empty, prism, sphere


def hut():
    r = empty("hut")
    box((1.6, 1.4, 0.9), (0, 0, 0.45), "plaster", parent=r, bevel=0.03)
    prism((1.6, 1.4, 0.6), (0, 0, 0.9), "terracotta", parent=r, overhang=0.12)
    box((0.3, 0.05, 0.55), (0.3, -0.71, 0.28), "wood_dark", parent=r)
    cyl(0.07, 0.7, (1.3, -0.6, 0.35), "wood", parent=r)
    sphere(0.4, (1.3, -0.6, 0.95), "leaf_olive", parent=r)
    return r
