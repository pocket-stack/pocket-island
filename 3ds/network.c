/* Frame-boundary copies only. The shared offload worker owns network IO. */
#include "network.h"
#include "offload.h"
#include <string.h>
typedef struct NativeNet NativeNet;
extern NativeNet *island_net_new(const Island *);
extern void island_net_free(NativeNet *);
extern void island_net_frame(NativeNet *, int session);
extern void island_net_receive(NativeNet *, const uint8_t *, uint32_t);
extern uint32_t island_net_outgoing(NativeNet *, uint8_t *out);
extern void island_net_sent(NativeNet *);
extern void island_net_step(NativeNet *, Island *, float, float, uint32_t);
extern Island *island_net_actor(NativeNet *);
extern void island_net_snapshot(const NativeNet *, IslandNetworkSnapshot *);
static NativeNet *network;
static char incoming[4096];
static uint8_t outgoing[512];
bool island_network_init(const Island *island) {
#ifdef ISLAND_CAPTURE
  (void)island;
  return true;
#else
  network = island_net_new(island);
  return network && offload_start();
#endif
}
void island_network_poll(bool suspended) {
  if (!network) return;
  offload_frame();
  island_net_frame(network, suspended ? 0 : offload_session());
  size_t n = offload_take(incoming);
  if (n && !suspended) island_net_receive(network, (const uint8_t *)incoming, n);
  n = island_net_outgoing(network, outgoing);
  if (n && offload_submit((const char *)outgoing, n)) island_net_sent(network);
}
void island_network_step(Island *island, float x, float z, uint32_t flags) {
  if (network) island_net_step(network, island, x, z, flags);
  else island_step(island, x, z, flags);
}
Island *island_network_remote(void) { return network ? island_net_actor(network) : NULL; }
void island_network_snapshot(IslandNetworkSnapshot *out) {
  memset(out, 0, sizeof *out);
  if (network) island_net_snapshot(network, out);
}
void island_network_shutdown(void) {
  if (!network) return;
  offload_stop();
  island_net_free(network); network = NULL;
}
