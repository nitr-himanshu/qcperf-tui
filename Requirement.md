# Requirements

Build a terminal UI on libqcperf that displays live metric values and graphs. On launch, initialize libqcperf, discover its capabilities, and create a default dashboard. Users can create dashboards and choose which metrics to profile. Each metric uses the sampling and streaming rates reported by its capability.

Each live dashboard window shows one graph at a time. The user can select any configured metric and choose either a line graph or a bar graph; pie graphs are not used. Graphs show a rolling time window, with new samples entering at the right and older samples moving left. Set the y-axis range after collecting two finite samples. Limit plotted samples by the available horizontal pixel count divided by the pixels required for each sample.

Dashboards support editing, start and stop, snapshots, and CSV export. Switching dashboards does not stop profiling, and multiple dashboards can run together. The terminal layout scales to the available window size. Provide default graph colors and let the user configure them.

Build for ARM GNU Linux, Android, and Windows ARM64 (MSVC).
