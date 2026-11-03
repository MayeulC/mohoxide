/*
 * pl_mpeg (phoboslab, MIT; `pl_mpeg.h` from github.com/phoboslab/pl_mpeg,
 * unmodified) built for the engine's movie decoder (`engine::assets::mpeg`).
 *
 * The data comes from Rust through read / seek / tell callbacks, so a movie
 * streams from its file 128 KB at a time (SupCom's intro is 177 MB) and the
 * C side never touches the file system: no stdio, no `fopen` (MSVC's
 * deprecation warnings, non-ASCII paths on Windows). Plain C89/C99: builds
 * with MSVC, clang and gcc.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define PLM_NO_STDIO
#define PL_MPEG_IMPLEMENTATION
#include "pl_mpeg.h"

/* Matches `RsSource` in src/engine/assets/mpeg.rs. */
typedef struct {
    size_t (*read)(void *user, uint8_t *dst, size_t len);
    void (*seek)(void *user, size_t offset);
    size_t (*tell)(void *user);
    void *user;
} plm_rs_source;

/* Like pl_mpeg's own file loader: drop what was read, append what's next. */
static void plm_rs_load(plm_buffer_t *self, void *user) {
    plm_rs_source *src = (plm_rs_source *)user;
    size_t n;
    if (self->discard_read_bytes) {
        plm_buffer_discard_read_bytes(self);
    }
    n = src->read(src->user, self->bytes + self->length, self->capacity - self->length);
    self->length += n;
    if (n == 0) {
        self->has_ended = TRUE;
    }
}

static void plm_rs_seek(plm_buffer_t *self, size_t offset, void *user) {
    plm_rs_source *src = (plm_rs_source *)user;
    PLM_UNUSED(self);
    src->seek(src->user, offset);
}

static size_t plm_rs_tell(plm_buffer_t *self, void *user) {
    plm_rs_source *src = (plm_rs_source *)user;
    PLM_UNUSED(self);
    return src->tell(src->user);
}

/*
 * A decoder over `src` (`length` bytes; `src` must outlive it), video only.
 * NULL unless it's an MPEG-1 program stream with a video stream.
 */
plm_t *plm_rs_create(plm_rs_source *src, size_t length) {
    plm_buffer_t *buffer = plm_buffer_create_with_callbacks(plm_rs_load, plm_rs_seek, plm_rs_tell, length, src);
    plm_t *plm;
    /* pl_mpeg's callback buffers keep every byte read; stream instead. */
    buffer->discard_read_bytes = TRUE;
    plm = plm_create_with_buffer(buffer, TRUE);
    if (!plm_has_headers(plm) || plm_get_num_video_streams(plm) < 1) {
        plm_destroy(plm);
        return NULL;
    }
    plm_set_audio_enabled(plm, FALSE);
    return plm;
}
