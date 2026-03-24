"""CLI commands for Nvidia-SMI Manager"""

import click
from rich.console import Console
from rich.table import Table
from rich.live import Live
from rich.layout import Layout
import time
import sys
from pathlib import Path

from nv_smi_manager.core.gpu_monitor import GPUMonitor
from nv_smi_manager.core.system_info import SystemMonitor
from nv_smi_manager.core.config import Config
from nv_smi_manager.core.gpu_control import GPUController
from nv_smi_manager.utils.formatters import format_memory, format_power, format_temperature

console = Console()


@click.group()
@click.version_option()
def main():
    """NV-SMI Manager - GPU monitoring and management for NixOS"""
    pass


@main.command()
@click.option('--watch', is_flag=True, help='Enable watch mode for continuous monitoring')
@click.option('--interval', type=int, default=2, help='Refresh interval in seconds')
def status(watch: bool, interval: int):
    """Display GPU status and information"""
    monitor = GPUMonitor()
    
    if not watch:
        _display_status(monitor)
    else:
        try:
            with Live(_build_status_layout(monitor), refresh_per_second=1) as live:
                while True:
                    monitor._update_gpu_info()
                    live.update(_build_status_layout(monitor))
                    time.sleep(interval)
        except KeyboardInterrupt:
            console.print("\n[yellow]Monitoring stopped[/yellow]")


def _display_status(monitor: GPUMonitor):
    """Display status in a single view"""
    gpus = monitor.get_all_gpus()
    
    if not gpus:
        console.print("[red]No NVIDIA GPUs found[/red]")
        return
    
    table = Table(title="GPU Status", show_header=True, header_style="bold magenta")
    table.add_column("GPU", style="cyan", width=5)
    table.add_column("Name", style="cyan")
    table.add_column("Memory", style="green")
    table.add_column("Temp", style="yellow")
    table.add_column("Power", style="yellow")
    table.add_column("Util", style="blue")
    table.add_column("Core Offset (MHz)", style="magenta")
    table.add_column("Mem Offset (MHz)", style="magenta")
    
    for gpu in gpus:
        memory_str = f"{gpu.memory_used}/{gpu.memory_total} MB"
        temp_str = f"{gpu.temperature}°C" if gpu.temperature else "N/A"
        power_str = format_power(gpu.power_draw, gpu.power_limit)
        util_str = f"{gpu.utilization}%" if gpu.utilization else "N/A"
        core_offset_str = str(gpu.core_clock_offset) if gpu.core_clock_offset is not None else "N/A"
        mem_offset_str = str(gpu.memory_clock_offset) if gpu.memory_clock_offset is not None else "N/A"
        
        table.add_row(
            str(gpu.index),
            gpu.name,
            memory_str,
            temp_str,
            power_str,
            util_str,
            core_offset_str,
            mem_offset_str
        )
    
    console.print(table)


def _build_status_layout(monitor: GPUMonitor) -> Layout:
    """Build layout for watch mode"""
    layout = Layout()
    layout.split_column(
        Layout(name="header", size=3),
        Layout(name="body"),
        Layout(name="footer", size=3)
    )
    
    gpus = monitor.get_all_gpus()
    table_content = _build_gpu_table(gpus)
    
    layout["header"].update(
        f"[bold blue]NV-SMI Manager[/bold blue] - Press Ctrl+C to exit"
    )
    layout["body"].update(table_content)
    layout["footer"].update(
        f"[dim]Updated at {time.strftime('%H:%M:%S')}[/dim]"
    )
    
    return layout


def _build_gpu_table(gpus) -> Table:
    """Build GPU status table"""
    table = Table(show_header=True, header_style="bold magenta")
    table.add_column("GPU", style="cyan", width=5)
    table.add_column("Name", style="cyan", width=20)
    table.add_column("Memory", style="green", width=15)
    table.add_column("Temp", style="yellow", width=10)
    table.add_column("Power", style="yellow", width=15)
    table.add_column("Util", style="blue", width=10)
    table.add_column("Core Offset", style="magenta", width=12)
    table.add_column("Mem Offset", style="magenta", width=12)
    
    for gpu in gpus:
        memory_str = f"{gpu.memory_used}/{gpu.memory_total} MB"
        temp_str = f"{gpu.temperature}°C" if gpu.temperature else "N/A"
        power_str = format_power(gpu.power_draw, gpu.power_limit)
        util_str = f"{gpu.utilization}%" if gpu.utilization else "N/A"
        core_offset_str = str(gpu.core_clock_offset) if gpu.core_clock_offset is not None else "N/A"
        mem_offset_str = str(gpu.memory_clock_offset) if gpu.memory_clock_offset is not None else "N/A"
        
        table.add_row(
            str(gpu.index),
            gpu.name,
            memory_str,
            temp_str,
            power_str,
            util_str,
            core_offset_str,
            mem_offset_str
        )
    
    return table


@main.command()
def system():
    """Display system information"""
    sys_monitor = SystemMonitor()
    sys_info = sys_monitor.get_system_info()
    cpu_info = sys_monitor.get_cpu_count()
    mem_info = sys_monitor.get_memory_info()
    
    table = Table(title="System Information", show_header=True, header_style="bold magenta")
    table.add_column("Property", style="cyan")
    table.add_column("Value", style="green")
    
    table.add_row("CPU Cores (Physical)", str(cpu_info['physical']))
    table.add_row("CPU Cores (Logical)", str(cpu_info['logical']))
    table.add_row("CPU Usage", f"{sys_info.cpu_percent}%")
    table.add_row("Memory Total", format_memory(mem_info['total']))
    table.add_row("Memory Used", format_memory(mem_info['used']))
    table.add_row("Memory Available", format_memory(mem_info['available']))
    table.add_row("Memory Usage", f"{sys_info.memory_percent}%")
    table.add_row("Disk Usage", f"{sys_info.disk_percent}%")
    table.add_row("Load Average (1m, 5m, 15m)", 
                  f"{sys_info.load_average[0]:.2f}, {sys_info.load_average[1]:.2f}, {sys_info.load_average[2]:.2f}")
    
    console.print(table)


@main.command()
@click.option('--gpu', type=int, default=0, help='GPU index to query')
def info(gpu: int):
    """Display detailed information about a specific GPU"""
    monitor = GPUMonitor()
    gpu_info = monitor.get_gpu(gpu)
    
    if not gpu_info:
        console.print(f"[red]GPU {gpu} not found[/red]")
        return
    
    table = Table(title=f"GPU {gpu} Details", show_header=True, header_style="bold magenta")
    table.add_column("Property", style="cyan")
    table.add_column("Value", style="green")
    
    table.add_row("Index", str(gpu_info.index))
    table.add_row("Name", gpu_info.name)
    table.add_row("Memory Total", format_memory(gpu_info.memory_total * 1024 * 1024))
    table.add_row("Memory Used", format_memory(gpu_info.memory_used * 1024 * 1024))
    table.add_row("Memory Free", format_memory(gpu_info.memory_free * 1024 * 1024))
    table.add_row("Temperature", format_temperature(gpu_info.temperature))
    table.add_row("Power Draw", format_power(gpu_info.power_draw, gpu_info.power_limit))
    table.add_row("Utilization", f"{gpu_info.utilization}%" if gpu_info.utilization else "N/A")
    
    console.print(table)


@main.command()
def config():
    """Manage configuration"""
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    
    click.echo(f"Configuration file: {config_path}")
    click.echo(f"Refresh interval: {config.refresh_interval}s")
    click.echo(f"Update frequency: {config.update_frequency}")
    click.echo(f"Log level: {config.log_level}")
    click.echo(f"Alerts enabled: {config.enable_alerts}")
    click.echo(f"Temperature threshold: {config.temp_threshold}°C")
    click.echo(f"Power threshold: {config.power_threshold}%")


# GPU Control Commands


@main.command()
@click.option('--gpu', type=int, required=True, help='GPU index')
@click.option('--core-offset', type=int, default=None, help='Core clock offset in MHz')
@click.option('--memory-offset', type=int, default=None, help='Memory clock offset in MHz')
def overclock(gpu: int, core_offset: int, memory_offset: int):
    """Overclock GPU (increase clock speeds)"""
    try:
        controller = GPUController()
    except RuntimeError as e:
        console.print(f"[red]Error: {e}[/red]")
        return
    
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    
    success = True
    if core_offset is not None:
        console.print(f"[yellow]Setting GPU {gpu} core clock offset to {core_offset} MHz...[/yellow]")
        if controller.set_core_clock_offset(gpu, core_offset):
            console.print(f"[green]✓ Core clock offset set to {core_offset} MHz[/green]")
        else:
            console.print(f"[red]✗ Failed to set core clock offset[/red]")
            success = False
    
    if memory_offset is not None:
        console.print(f"[yellow]Setting GPU {gpu} memory clock offset to {memory_offset} MHz...[/yellow]")
        if controller.set_memory_clock_offset(gpu, memory_offset):
            console.print(f"[green]✓ Memory clock offset set to {memory_offset} MHz[/green]")
        else:
            console.print(f"[red]✗ Failed to set memory clock offset[/red]")
            success = False
    
    if success:
        # Save profile to config
        profile = config.get_gpu_profile(gpu) or {}
        if core_offset is not None:
            profile['core_clock_offset'] = core_offset
        if memory_offset is not None:
            profile['memory_clock_offset'] = memory_offset
        
        config.set_gpu_profile(gpu, profile)
        config.save_to_file(config_path)
        console.print(f"[green]✓ Profile saved to {config_path}[/green]")


@main.command()
@click.option('--gpu', type=int, required=True, help='GPU index')
@click.option('--power-limit', type=int, required=True, help='Power limit in Watts')
def undervolt(gpu: int, power_limit: int):
    """Undervolt GPU (reduce power limit for lower voltage)"""
    try:
        controller = GPUController()
    except RuntimeError as e:
        console.print(f"[red]Error: {e}[/red]")
        return
    
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    
    console.print(f"[yellow]Setting GPU {gpu} power limit to {power_limit} W...[/yellow]")
    if controller.set_power_limit(gpu, power_limit):
        console.print(f"[green]✓ Power limit set to {power_limit} W[/green]")
        
        # Save profile to config
        profile = config.get_gpu_profile(gpu) or {}
        profile['power_limit'] = power_limit
        config.set_gpu_profile(gpu, profile)
        config.save_to_file(config_path)
        console.print(f"[green]✓ Profile saved to {config_path}[/green]")
    else:
        console.print(f"[red]✗ Failed to set power limit[/red]")


@main.command()
@click.option('--gpu', type=int, required=True, help='GPU index')
def reset_gpu(gpu: int):
    """Reset GPU to default settings (all offsets to 0)"""
    try:
        controller = GPUController()
    except RuntimeError as e:
        console.print(f"[red]Error: {e}[/red]")
        return
    
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    
    console.print(f"[yellow]Resetting GPU {gpu} to default settings...[/yellow]")
    if controller.reset_gpu(gpu):
        console.print(f"[green]✓ GPU {gpu} reset to default settings[/green]")
        
        # Clear profile from config
        if str(gpu) in config.gpu_profiles:
            del config.gpu_profiles[str(gpu)]
            config.save_to_file(config_path)
            console.print(f"[green]✓ Profile removed from {config_path}[/green]")
    else:
        console.print(f"[red]✗ Failed to reset GPU {gpu}[/red]")


@main.command()
def apply_profile():
    """Apply saved GPU profiles from configuration"""
    try:
        controller = GPUController()
    except RuntimeError as e:
        console.print(f"[red]Error: {e}[/red]")
        return
    
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    profiles = config.get_all_gpu_profiles()
    
    if not profiles:
        console.print("[yellow]No GPU profiles found in configuration[/yellow]")
        return
    
    console.print(f"[bold]Applying {len(profiles)} GPU profile(s)...[/bold]")
    
    for gpu_index_str, profile in profiles.items():
        gpu_index = int(gpu_index_str)
        
        if 'core_clock_offset' in profile:
            offset = profile['core_clock_offset']
            controller.set_core_clock_offset(gpu_index, offset)
            console.print(f"  GPU {gpu_index}: Core clock offset = {offset} MHz")
        
        if 'memory_clock_offset' in profile:
            offset = profile['memory_clock_offset']
            controller.set_memory_clock_offset(gpu_index, offset)
            console.print(f"  GPU {gpu_index}: Memory clock offset = {offset} MHz")
        
        if 'power_limit' in profile:
            limit = profile['power_limit']
            controller.set_power_limit(gpu_index, limit)
            console.print(f"  GPU {gpu_index}: Power limit = {limit} W")
    
    console.print("[green]✓ All profiles applied[/green]")


@main.command()
def profile_list():
    """List all saved GPU profiles"""
    config_path = Config.get_default_config_path()
    config = Config.from_file(config_path)
    profiles = config.get_all_gpu_profiles()
    
    if not profiles:
        console.print("[yellow]No GPU profiles saved[/yellow]")
        return
    
    table = Table(title="Saved GPU Profiles", show_header=True, header_style="bold magenta")
    table.add_column("GPU", style="cyan", width=5)
    table.add_column("Core Clock Offset (MHz)", style="green")
    table.add_column("Memory Clock Offset (MHz)", style="green")
    table.add_column("Power Limit (W)", style="yellow")
    
    for gpu_index_str, profile in sorted(profiles.items()):
        core_offset = profile.get('core_clock_offset', 'N/A')
        memory_offset = profile.get('memory_clock_offset', 'N/A')
        power_limit = profile.get('power_limit', 'N/A')
        
        table.add_row(
            gpu_index_str,
            str(core_offset),
            str(memory_offset),
            str(power_limit)
        )
    
    console.print(table)
    console.print(f"\n[dim]Config file: {config_path}[/dim]")


if __name__ == '__main__':
    main()
