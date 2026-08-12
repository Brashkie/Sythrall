"""
conftest.py — configura el path para que pytest importe correctamente el backend.
"""

import sys
from pathlib import Path

# Agregar el backend al sys.path
sys.path.insert(0, str(Path(__file__).parent.parent))
