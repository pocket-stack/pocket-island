#ifndef ISLAND_NETWORK_H
#define ISLAND_NETWORK_H
#include "island.h"
typedef struct {
  uint32_t linked, player, remote, pending, acknowledged, predicted;
  uint32_t corrections, replayed, rejected, stalled;
  float remote_x, remote_z;
  uint32_t remote_tick;
} IslandNetworkSnapshot;
_Static_assert(sizeof(IslandNetworkSnapshot) == 52, "Network snapshot ABI");
bool island_network_init(const Island *);
void island_network_shutdown(void);
void island_network_poll(bool suspended);
void island_network_step(Island *, float x, float z, uint32_t flags);
Island *island_network_remote(void);
void island_network_snapshot(IslandNetworkSnapshot *);
#endif
