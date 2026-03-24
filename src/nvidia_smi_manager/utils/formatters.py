"""Output formatting utilities"""

from typing import Optional


def format_memory(bytes_val: Optional[int]) -> str:
    """Format byte values to human-readable format"""
    if bytes_val is None:
        return "N/A"
    
    units = ['B', 'KB', 'MB', 'GB', 'TB']
    size = float(bytes_val)
    
    for unit in units:
        if size < 1024.0:
            return f"{size:.2f} {unit}"
        size /= 1024.0
    
    return f"{size:.2f} PB"


def format_power(power_draw: Optional[float], power_limit: Optional[float]) -> str:
    """Format power draw with limit"""
    if power_draw is None:
        return "N/A"
    
    if power_limit is None:
        return f"{power_draw:.1f} W"
    
    percent = (power_draw / power_limit) * 100 if power_limit > 0 else 0
    return f"{power_draw:.1f}/{power_limit:.1f} W ({percent:.1f}%)"


def format_temperature(temp: Optional[float]) -> str:
    """Format temperature"""
    if temp is None:
        return "N/A"
    
    return f"{temp:.1f}°C"


def format_percentage(percent: Optional[float]) -> str:
    """Format percentage"""
    if percent is None:
        return "N/A"
    
    return f"{percent:.1f}%"
