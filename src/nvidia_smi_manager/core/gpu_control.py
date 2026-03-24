"""GPU control operations (overclocking, undervolting)"""

import subprocess
from dataclasses import dataclass
from typing import Optional, Dict, Tuple
import re


@dataclass
class GPUProfile:
    """GPU performance profile with clock offsets and power limits"""
    gpu_index: int
    core_clock_offset: int = 0  # MHz, can be negative for underclocking
    memory_clock_offset: int = 0  # MHz
    power_limit: Optional[int] = None  # Watts


class GPUController:
    """Control NVIDIA GPU settings via nvidia-settings"""

    # Default safety limits per GPU
    DEFAULT_LIMITS = {
        "core_clock_range": (-75, 100),  # MHz
        "memory_clock_range": (-75, 100),  # MHz
        "power_limit_range": (0, 500),  # Watts (0 = no limit)
    }

    def __init__(self, safety_limits: Optional[Dict] = None):
        """Initialize GPU controller with optional custom safety limits"""
        self.safety_limits = safety_limits or self.DEFAULT_LIMITS
        self._check_nvidia_settings()

    @staticmethod
    def _check_nvidia_settings() -> bool:
        """Check if nvidia-settings is available"""
        try:
            subprocess.run(
                ["nvidia-settings", "--version"],
                capture_output=True,
                check=True,
                timeout=5
            )
            return True
        except (subprocess.CalledProcessError, FileNotFoundError):
            raise RuntimeError(
                "nvidia-settings not found. Please install nvidia-settings: "
                "sudo apt-get install nvidia-settings  (Debian/Ubuntu) or "
                "nix-shell with nvidia-settings in devShell"
            )

    def set_core_clock_offset(
        self, gpu_index: int, offset_mhz: int, validate: bool = True
    ) -> bool:
        """
        Set GPU core clock offset
        
        Args:
            gpu_index: GPU index
            offset_mhz: Clock offset in MHz (can be negative for underclocking)
            validate: Check against safety limits
            
        Returns:
            True if successful, False otherwise
        """
        if validate:
            min_val, max_val = self.safety_limits["core_clock_range"]
            if not (min_val <= offset_mhz <= max_val):
                raise ValueError(
                    f"Core clock offset {offset_mhz} MHz out of bounds "
                    f"[{min_val}, {max_val}]"
                )

        return self._apply_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUGraphicsClockOffset",
            str(offset_mhz)
        )

    def set_memory_clock_offset(
        self, gpu_index: int, offset_mhz: int, validate: bool = True
    ) -> bool:
        """
        Set GPU memory clock offset
        
        Args:
            gpu_index: GPU index
            offset_mhz: Clock offset in MHz (can be negative for underclocking)
            validate: Check against safety limits
            
        Returns:
            True if successful, False otherwise
        """
        if validate:
            min_val, max_val = self.safety_limits["memory_clock_range"]
            if not (min_val <= offset_mhz <= max_val):
                raise ValueError(
                    f"Memory clock offset {offset_mhz} MHz out of bounds "
                    f"[{min_val}, {max_val}]"
                )

        return self._apply_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUMemoryTransferRateOffset",
            str(offset_mhz)
        )

    def set_power_limit(
        self, gpu_index: int, limit_watts: int, validate: bool = True
    ) -> bool:
        """
        Set GPU power limit (undervolting via power reduction)
        
        Args:
            gpu_index: GPU index
            limit_watts: Power limit in Watts
            validate: Check against safety limits
            
        Returns:
            True if successful, False otherwise
        """
        if validate:
            min_val, max_val = self.safety_limits["power_limit_range"]
            if not (min_val <= limit_watts <= max_val):
                raise ValueError(
                    f"Power limit {limit_watts} W out of bounds "
                    f"[{min_val}, {max_val}]"
                )

        return self._apply_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUPowerMizerMode",
            str(limit_watts)
        )

    def get_core_clock_offset(self, gpu_index: int) -> Optional[int]:
        """
        Get current GPU core clock offset
        
        Args:
            gpu_index: GPU index
            
        Returns:
            Clock offset in MHz, or None if not available
        """
        return self._query_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUGraphicsClockOffset"
        )

    def get_memory_clock_offset(self, gpu_index: int) -> Optional[int]:
        """
        Get current GPU memory clock offset
        
        Args:
            gpu_index: GPU index
            
        Returns:
            Clock offset in MHz, or None if not available
        """
        return self._query_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUMemoryTransferRateOffset"
        )

    def get_power_limit(self, gpu_index: int) -> Optional[int]:
        """
        Get current GPU power limit
        
        Args:
            gpu_index: GPU index
            
        Returns:
            Power limit in Watts, or None if not available
        """
        return self._query_nvidia_settings(
            f"[gpu:{gpu_index}]/GPUPowerMizerMode"
        )

    def apply_profile(self, profile: GPUProfile) -> bool:
        """
        Apply a complete GPU profile (all clock offsets and power limits)
        
        Args:
            profile: GPUProfile with desired settings
            
        Returns:
            True if all settings applied successfully
        """
        success = True

        if profile.core_clock_offset != 0:
            if not self.set_core_clock_offset(profile.gpu_index, profile.core_clock_offset):
                success = False

        if profile.memory_clock_offset != 0:
            if not self.set_memory_clock_offset(profile.gpu_index, profile.memory_clock_offset):
                success = False

        if profile.power_limit is not None:
            if not self.set_power_limit(profile.gpu_index, profile.power_limit):
                success = False

        return success

    def reset_gpu(self, gpu_index: int) -> bool:
        """
        Reset GPU to default settings (all offsets to 0)
        
        Args:
            gpu_index: GPU index
            
        Returns:
            True if successful
        """
        success = True
        
        if not self.set_core_clock_offset(gpu_index, 0, validate=False):
            success = False
        if not self.set_memory_clock_offset(gpu_index, 0, validate=False):
            success = False
        
        return success

    @staticmethod
    def _apply_nvidia_settings(attr: str, value: str) -> bool:
        """
        Apply a setting via nvidia-settings
        
        Args:
            attr: nvidia-settings attribute path (e.g., "[gpu:0]/GPUGraphicsClockOffset")
            value: Value to set
            
        Returns:
            True if successful, False otherwise
        """
        try:
            result = subprocess.run(
                ["nvidia-settings", "-a", f"{attr}={value}"],
                capture_output=True,
                text=True,
                check=False,
                timeout=10
            )
            
            if result.returncode == 0:
                return True
            else:
                if "Permission denied" in result.stderr or "Permission denied" in result.stdout:
                    print(
                        f"Permission denied. Please run with elevated privileges: "
                        f"sudo nvidia-smi-manager ..."
                    )
                else:
                    print(f"Failed to set {attr}={value}: {result.stderr}")
                return False
        except subprocess.TimeoutExpired:
            print(f"Timeout setting {attr}={value}")
            return False
        except FileNotFoundError:
            print("nvidia-settings not found")
            return False

    @staticmethod
    def _query_nvidia_settings(attr: str) -> Optional[int]:
        """
        Query a setting value via nvidia-settings
        
        Args:
            attr: nvidia-settings attribute path
            
        Returns:
            Integer value, or None if not available
        """
        try:
            result = subprocess.run(
                ["nvidia-settings", "-q", attr],
                capture_output=True,
                text=True,
                check=False,
                timeout=10
            )
            
            if result.returncode == 0:
                # Parse output like "Attribute 'GPUGraphicsClockOffset' (nvidia1:0): 50."
                match = re.search(r":\s*(-?\d+)", result.stdout)
                if match:
                    return int(match.group(1))
            return None
        except (subprocess.TimeoutExpired, FileNotFoundError, ValueError):
            return None
