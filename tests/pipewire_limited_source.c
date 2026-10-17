/* SPDX-License-Identifier: MIT
 * Generated BGRA frames only. Models a compositor offering 2..4 buffers;
 * no screen, camera or audio access. Run against the private test daemon.
 */
#define _GNU_SOURCE
#include <pipewire/pipewire.h>
#include <spa/param/video/format-utils.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#include <errno.h>

struct source {
    struct pw_main_loop *loop;
    struct pw_stream *stream;
    struct spa_source *timer;
    struct spa_video_info_raw format;
    uint32_t stride, seq;
    uint32_t allocated;
};

struct allocation { void *mapping; size_t size; int fd; };

static void add_buffer(void *userdata,struct pw_buffer *buffer)
{
    struct source *s=userdata;
    if (s->allocated>=4) { pw_stream_set_error(s->stream,-EINVAL,"Synthetic producer allows at most four buffers"); return; }
    uint64_t size=(uint64_t)s->stride*s->format.size.height;
    if (!size || size>128*1024*1024 || buffer->buffer->n_datas!=1) {
        pw_stream_set_error(s->stream,-EINVAL,"Invalid synthetic buffer size");
        return;
    }
    struct spa_data *d=buffer->buffer->datas;
    if (!(d[0].type&(1<<SPA_DATA_MemFd))) {
        pw_stream_set_error(s->stream,-EINVAL,"Synthetic producer requires MemFd");
        return;
    }
    struct allocation *a=calloc(1,sizeof(*a));
    if (!a) { pw_stream_set_error(s->stream,-ENOMEM,"Synthetic allocation failed"); return; }
    a->size=(size_t)size;
    a->fd=memfd_create("hypercast-synthetic",MFD_CLOEXEC|MFD_ALLOW_SEALING);
    if (a->fd<0 || ftruncate(a->fd,(off_t)size)<0) {
        if (a->fd>=0) close(a->fd);
        free(a);
        pw_stream_set_error(s->stream,-ENOMEM,"Synthetic MemFd failed");
        return;
    }
    a->mapping=mmap(NULL,a->size,PROT_READ|PROT_WRITE,MAP_SHARED,a->fd,0);
    if (a->mapping==MAP_FAILED) {
        close(a->fd); free(a);
        pw_stream_set_error(s->stream,-ENOMEM,"Synthetic mapping failed");
        return;
    }
    buffer->user_data=a;
    ++s->allocated;
    d[0].type=SPA_DATA_MemFd;
    d[0].fd=a->fd;
    d[0].mapoffset=0;
    d[0].maxsize=(uint32_t)size;
    d[0].data=a->mapping;
}

static void remove_buffer(void *userdata,struct pw_buffer *buffer)
{
    struct source *s=userdata;
    struct allocation *a=buffer->user_data;
    buffer->user_data=NULL;
    if (a) { munmap(a->mapping,a->size); close(a->fd); free(a); --s->allocated; }
}

static void process(void *userdata)
{
    struct source *s=userdata;
    struct pw_buffer *b=pw_stream_dequeue_buffer(s->stream);
    if (!b) return;
    struct spa_buffer *buffer=b->buffer;
    uint64_t size=(uint64_t)s->stride*s->format.size.height;
    if (buffer->n_datas==1 && buffer->datas[0].data && size<=buffer->datas[0].maxsize) {
        struct spa_data *d=&buffer->datas[0];
        memset(d->data,(s->seq%64)+64,(size_t)size);
        d->chunk->offset=0;
        d->chunk->size=(uint32_t)size;
        d->chunk->stride=s->stride;
        struct spa_meta_header *header=spa_buffer_find_meta_data(buffer,SPA_META_Header,sizeof(*header));
        if (header) {
            header->seq=s->seq++;
            header->pts=pw_stream_get_nsec(s->stream);
            header->dts_offset=0;
            header->flags=0;
        }
    }
    pw_stream_queue_buffer(s->stream,b);
}

static void tick(void *userdata,uint64_t expirations)
{
    struct source *s=userdata;
    (void)expirations;
    pw_stream_trigger_process(s->stream);
}

static void state(void *userdata,enum pw_stream_state old,enum pw_stream_state current,const char *error)
{
    struct source *s=userdata;
    (void)old;
    struct timespec first={0,1}, interval={0,33333333};
    pw_loop_update_timer(pw_main_loop_get_loop(s->loop),s->timer,
        current==PW_STREAM_STATE_STREAMING?&first:NULL,
        current==PW_STREAM_STATE_STREAMING?&interval:NULL,false);
    if (current==PW_STREAM_STATE_ERROR) fprintf(stderr,"Synthetic producer: %s\n",error?error:"stream failed");
}

static void format(void *userdata,uint32_t id,const struct spa_pod *param)
{
    struct source *s=userdata;
    if (id!=SPA_PARAM_Format || !param) return;
    if (spa_format_video_raw_parse(param,&s->format)<0) return;
    if (!s->format.size.width || s->format.size.width>8192 || !s->format.size.height || s->format.size.height>8192) return;
    s->stride=s->format.size.width*4;
    uint8_t bytes[512];
    struct spa_pod_builder builder=SPA_POD_BUILDER_INIT(bytes,sizeof(bytes));
    const struct spa_pod *params[2];
    params[0]=spa_pod_builder_add_object(&builder,SPA_TYPE_OBJECT_ParamBuffers,SPA_PARAM_Buffers,
        SPA_PARAM_BUFFERS_buffers,SPA_POD_CHOICE_RANGE_Int(3,2,4),
        SPA_PARAM_BUFFERS_blocks,SPA_POD_Int(1),
        SPA_PARAM_BUFFERS_size,SPA_POD_Int(s->stride*s->format.size.height),
        SPA_PARAM_BUFFERS_stride,SPA_POD_Int(s->stride),
        SPA_PARAM_BUFFERS_dataType,SPA_POD_CHOICE_FLAGS_Int(1<<SPA_DATA_MemFd));
    params[1]=spa_pod_builder_add_object(&builder,SPA_TYPE_OBJECT_ParamMeta,SPA_PARAM_Meta,
        SPA_PARAM_META_type,SPA_POD_Id(SPA_META_Header),
        SPA_PARAM_META_size,SPA_POD_Int(sizeof(struct spa_meta_header)));
    pw_stream_update_params(s->stream,params,2);
}

static void quit(void *userdata,int signal)
{
    struct source *s=userdata;
    (void)signal;
    pw_main_loop_quit(s->loop);
}

int main(int argc,char **argv)
{
    pw_init(&argc,&argv);
    struct source s={0};
    int result=-ENOMEM;
    s.loop=pw_main_loop_new(NULL);
    if (!s.loop) goto done;
    /* Block/register before PipeWire creates any data threads. */
    pw_loop_add_signal(pw_main_loop_get_loop(s.loop),SIGTERM,quit,&s);
    pw_loop_add_signal(pw_main_loop_get_loop(s.loop),SIGINT,quit,&s);
    s.timer=pw_loop_add_timer(pw_main_loop_get_loop(s.loop),tick,&s);
    if (!s.timer) goto cleanup;
    static const struct pw_stream_events events={PW_VERSION_STREAM_EVENTS,
        .process=process,.state_changed=state,.param_changed=format,.add_buffer=add_buffer,.remove_buffer=remove_buffer};
    s.stream=pw_stream_new_simple(pw_main_loop_get_loop(s.loop),"HyperCastTest",
        pw_properties_new(PW_KEY_NODE_NAME,"HyperCastTest",PW_KEY_MEDIA_CLASS,"Video/Source",NULL),&events,&s);
    if (!s.stream) goto cleanup;
    uint8_t bytes[512];
    struct spa_pod_builder builder=SPA_POD_BUILDER_INIT(bytes,sizeof(bytes));
    const struct spa_pod *param=spa_pod_builder_add_object(&builder,SPA_TYPE_OBJECT_Format,SPA_PARAM_EnumFormat,
        SPA_FORMAT_mediaType,SPA_POD_Id(SPA_MEDIA_TYPE_video),
        SPA_FORMAT_mediaSubtype,SPA_POD_Id(SPA_MEDIA_SUBTYPE_raw),
        SPA_FORMAT_VIDEO_format,SPA_POD_Id(SPA_VIDEO_FORMAT_BGRA),
        SPA_FORMAT_VIDEO_size,SPA_POD_CHOICE_RANGE_Rectangle(&SPA_RECTANGLE(3440,1440),&SPA_RECTANGLE(2,2),&SPA_RECTANGLE(8192,8192)),
        SPA_FORMAT_VIDEO_framerate,SPA_POD_Fraction(&SPA_FRACTION(0,1)),
        SPA_FORMAT_VIDEO_maxFramerate,SPA_POD_Fraction(&SPA_FRACTION(180,1)));
    result=pw_stream_connect(s.stream,PW_DIRECTION_OUTPUT,PW_ID_ANY,PW_STREAM_FLAG_DRIVER|PW_STREAM_FLAG_ALLOC_BUFFERS,&param,1);
    if (result>=0) result=pw_main_loop_run(s.loop);
cleanup:
    if (s.stream) pw_stream_destroy(s.stream);
    if (s.timer) pw_loop_destroy_source(pw_main_loop_get_loop(s.loop),s.timer);
    pw_main_loop_destroy(s.loop);
    if (s.allocated!=0) { fprintf(stderr,"Synthetic buffers retained: %u\n",s.allocated); result=-EFAULT; }
done:
    pw_deinit();
    return result<0?1:0;
}
