/* Raw-DEFLATE one-shot inflate through ISA-L, state reused across calls. */
#include <isa-l/igzip_lib.h>
#include <stdlib.h>
void *isal_shim_new(void) { struct inflate_state *s = malloc(sizeof *s); isal_inflate_init(s); return s; }
/* returns bytes written, or -1 on error / incomplete stream */
long isal_shim_raw(void *p, const unsigned char *in, unsigned in_len, unsigned char *out, unsigned out_len) {
    struct inflate_state *s = p;
    isal_inflate_reset(s);
    s->crc_flag = ISAL_DEFLATE;
    s->next_in = (unsigned char *)in; s->avail_in = in_len;
    s->next_out = out; s->avail_out = out_len;
    int r = isal_inflate(s);
    if (r != ISAL_DECOMP_OK || s->block_state != ISAL_BLOCK_FINISH) return -1;
    return (long)s->total_out;
}
